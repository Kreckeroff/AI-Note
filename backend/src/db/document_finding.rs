use sqlx::{FromRow, Postgres, Transaction};
use uuid::Uuid;

use super::{PageAddress, Store, sections::decode_title};
use crate::{
    document_path::page_path,
    text::{fold_name, markdown_to_text},
};

pub(crate) const SEARCH_PAGE_SIZE: i64 = 20;

#[derive(FromRow)]
struct ResultRow {
    id: Uuid,
    space_id: Uuid,
    page_title: Vec<u8>,
    space_title: Option<Vec<u8>>,
    section_title: Option<Vec<u8>>,
    source: String,
    task_number: Option<i64>,
}

pub(crate) struct ResultItem {
    pub id: Uuid,
    pub space_id: Uuid,
    pub title: String,
    pub space_title: Option<String>,
    pub section_title: Option<String>,
    pub snippet: String,
    pub task_number: Option<i64>,
}

pub(crate) struct SearchResults {
    pub items: Vec<ResultItem>,
    pub next: bool,
}

pub(crate) struct PageLinkMatch {
    pub id: Uuid,
    pub title: String,
    pub section_title: String,
}

pub(crate) struct BacklinkCandidate {
    pub id: Uuid,
    pub space_id: Uuid,
    pub title: String,
    pub markdown: String,
}

async fn can_search(
    tx: &mut Transaction<'_, Postgres>,
    tenant: Uuid,
    space: Option<Uuid>,
    account: Uuid,
) -> Result<bool, sqlx::Error> {
    sqlx::query_scalar(
        "SELECT EXISTS (
            SELECT 1 FROM tenant_memberships m
            WHERE (m.tenant_id, m.account_id) = ($1, $2) AND m.status = 'active'
              AND ($3::uuid IS NULL OR EXISTS (
                   SELECT 1 FROM spaces s
                   CROSS JOIN LATERAL page_capabilities(s.tenant_id, s.id, $2, NULL) access
                   WHERE (s.tenant_id, s.id) = ($1, $3) AND access.can_browse_space
              ))
        )",
    )
    .bind(tenant)
    .bind(account)
    .bind(space)
    .fetch_one(&mut **tx)
    .await
}

impl Store {
    pub(crate) async fn search_page_links(
        &self,
        tenant: Uuid,
        space: Uuid,
        account: Uuid,
        query: &str,
        target: Option<Uuid>,
        page: i64,
    ) -> Result<Option<Vec<PageLinkMatch>>, sqlx::Error> {
        let mut tx = self.0.begin().await?;
        sqlx::query("SET TRANSACTION ISOLATION LEVEL REPEATABLE READ READ ONLY")
            .execute(&mut *tx)
            .await?;
        sqlx::query("SET LOCAL statement_timeout = '4s'")
            .execute(&mut *tx)
            .await?;
        if !can_search(&mut tx, tenant, Some(space), account).await? {
            return Ok(None);
        }
        if query.is_empty() && target.is_none() {
            return Ok(Some(Vec::new()));
        }

        let matches: Vec<(Uuid, Vec<u8>, Vec<u8>)> = sqlx::query_as(
            "SELECT p.id, p.title, r.title
               FROM pages p
               JOIN sections r ON (r.tenant_id, r.space_id, r.id) = (p.tenant_id, p.space_id, p.section_id)
               CROSS JOIN LATERAL page_capabilities(p.tenant_id, p.space_id, $3, p.id) access
              WHERE p.tenant_id = $1 AND p.space_id = $2 AND access.can_read
                AND (p.id = $4 OR ($4::uuid IS NULL AND position($5 in p.title_key) > 0))
              ORDER BY p.title_key, r.title_key, p.id
              LIMIT $6 OFFSET $7",
        )
        .bind(tenant)
        .bind(space)
        .bind(account)
        .bind(target)
        .bind(fold_name(query).into_bytes())
        .bind(if target.is_some() { 1 } else { SEARCH_PAGE_SIZE + 1 })
        .bind(if target.is_some() { 0 } else { (page - 1) * SEARCH_PAGE_SIZE })
        .fetch_all(&mut *tx)
        .await?;
        tx.commit().await?;

        matches
            .into_iter()
            .map(|(id, title, section_title)| {
                Ok(PageLinkMatch {
                    id,
                    title: decode_title(title, "pages.title")?,
                    section_title: decode_title(section_title, "sections.title")?,
                })
            })
            .collect::<Result<Vec<_>, _>>()
            .map(Some)
    }

    pub(crate) async fn tasks_related_to_document(
        &self,
        current: &super::PageAddress,
    ) -> Result<Vec<(Uuid, i64, String, String)>, sqlx::Error> {
        sqlx::query_as(
            "SELECT t.id, t.number, t.title, t.status
               FROM workspace_tasks t
               CROSS JOIN LATERAL page_permissions($1, $2, $3, $4) document
               CROSS JOIN LATERAL page_capabilities($1, $2, $3, t.source_page_id) source
              WHERE (t.tenant_id, t.space_id) = ($1, $2)
                AND document.can_read AND document.can_browse_space
                AND source.can_browse_space AND (t.source_page_id IS NULL OR source.can_read)
                AND (t.source_page_id = $4 OR EXISTS (
                    SELECT 1 FROM workspace_approach_runs r
                     WHERE (r.tenant_id, r.space_id, r.task_id, r.result_page_id)
                           = ($1, $2, t.id, $4) AND r.status = 'accepted'
                ))
              ORDER BY t.number
              LIMIT 20",
        )
        .bind(current.tenant)
        .bind(current.space)
        .bind(current.account)
        .bind(current.page)
        .fetch_all(&self.0)
        .await
    }

    pub(crate) async fn search_documents(
        &self,
        tenant: Uuid,
        space: Option<Uuid>,
        account: Uuid,
        query: &str,
        page: i64,
    ) -> Result<Option<SearchResults>, sqlx::Error> {
        let mut tx = self.0.begin().await?;
        sqlx::query("SET TRANSACTION ISOLATION LEVEL REPEATABLE READ READ ONLY")
            .execute(&mut *tx)
            .await?;
        sqlx::query("SET LOCAL statement_timeout = '4s'")
            .execute(&mut *tx)
            .await?;
        if !can_search(&mut tx, tenant, space, account).await? {
            return Ok(None);
        }
        if query.is_empty() {
            return Ok(Some(SearchResults {
                items: Vec::new(),
                next: false,
            }));
        }
        let rows: Vec<ResultRow> = sqlx::query_as(
            "WITH matches AS (
                SELECT p.id, p.space_id, p.title AS page_title,
                       CASE WHEN access.can_browse_space THEN s.name END AS space_title,
                       CASE WHEN access.can_browse_space THEN r.title END AS section_title,
                       p.markdown AS source, NULL::bigint AS task_number,
                       position(convert_to($5, 'UTF8') in p.title_key) > 0 AS title_matches,
                       p.title_key
                  FROM pages p
                  JOIN spaces s ON (s.tenant_id, s.id) = (p.tenant_id, p.space_id)
                  JOIN sections r ON (r.tenant_id, r.space_id, r.id) = (p.tenant_id, p.space_id, p.section_id)
                  CROSS JOIN LATERAL page_capabilities(p.tenant_id, p.space_id, $2, p.id) access
                 WHERE p.tenant_id = $1 AND ($3::uuid IS NULL OR p.space_id = $3)
                   AND access.can_read
                   AND (position(convert_to($5, 'UTF8') in p.title_key) > 0
                       OR strpos(lower(normalize(p.markdown)), lower(normalize($4))) > 0)
                UNION ALL
                SELECT t.id, t.space_id, convert_to(t.title, 'UTF8'), s.name, NULL::bytea,
                       '', t.number, true, convert_to(lower(normalize(t.title)), 'UTF8')
                  FROM workspace_tasks t
                  JOIN spaces s ON (s.tenant_id,s.id) = (t.tenant_id,t.space_id)
                  CROSS JOIN LATERAL page_capabilities(t.tenant_id,t.space_id,$2,t.source_page_id) caps
                 WHERE t.tenant_id = $1 AND ($3::uuid IS NULL OR t.space_id = $3)
                   AND caps.can_browse_space AND (t.source_page_id IS NULL OR caps.can_read)
                   AND (strpos(lower(normalize(t.title)),lower(normalize($4))) > 0
                       OR strpos('kb-' || lpad(t.number::text,greatest(3,length(t.number::text)),'0'),lower($4)) > 0)
            )
            SELECT id,space_id,page_title,space_title,section_title,source,task_number FROM matches
            ORDER BY title_matches DESC,title_key,space_id,id
            LIMIT $6 OFFSET $7",
        )
        .bind(tenant)
        .bind(account)
        .bind(space)
        .bind(query)
        .bind(fold_name(query))
        .bind(SEARCH_PAGE_SIZE + 1)
        .bind((page - 1) * SEARCH_PAGE_SIZE)
        .fetch_all(&mut *tx)
        .await?;
        tx.commit().await?;

        let next = rows.len() > SEARCH_PAGE_SIZE as usize;
        let items = rows
            .into_iter()
            .take(SEARCH_PAGE_SIZE as usize)
            .map(|row| {
                Ok(ResultItem {
                    id: row.id,
                    space_id: row.space_id,
                    title: decode_title(row.page_title, "pages.title")?,
                    space_title: row
                        .space_title
                        .map(|title| decode_title(title, "spaces.name"))
                        .transpose()?,
                    section_title: row
                        .section_title
                        .map(|title| decode_title(title, "sections.title"))
                        .transpose()?,
                    snippet: excerpt(&row.source, query),
                    task_number: row.task_number,
                })
            })
            .collect::<Result<Vec<_>, sqlx::Error>>()?;
        Ok(Some(SearchResults { items, next }))
    }
}

fn excerpt(source: &str, query: &str) -> String {
    let source: String = markdown_to_text(source)
        .split_whitespace()
        .collect::<Vec<_>>()
        .join(" ");
    if source.is_empty() {
        return String::new();
    }
    let folded = fold_name(&source);
    let offset = folded
        .find(&fold_name(query))
        .map(|index| folded[..index].chars().count())
        .unwrap_or(0);
    let start = offset.saturating_sub(42);
    let fragment = source.chars().skip(start).take(165).collect::<String>();
    let prefix = if start > 0 { "…" } else { "" };
    let suffix = if source.chars().count() > start + 165 {
        "…"
    } else {
        ""
    };
    format!("{prefix}{fragment}{suffix}")
}

#[cfg(test)]
mod tests {
    use super::excerpt;

    #[test]
    fn excerpt_retains_a_russian_match_later_in_source() {
        let source = format!(
            "{} Поисковая фраза. Другой абзац.",
            "Перед текстом ".repeat(20)
        );
        let preview = excerpt(&source, "ПОИСКОВАЯ ФРАЗА");
        assert!(preview.starts_with('…'));
        assert!(preview.contains("Поисковая фраза"));
        assert!(preview.chars().count() < 168);
    }
}

impl Store {
    pub(crate) async fn backlink_candidates(
        &self,
        current: &PageAddress,
    ) -> Result<Vec<BacklinkCandidate>, sqlx::Error> {
        let target = page_path(current.tenant, current.space, current.page);
        let query = "SELECT p.space_id, p.id, p.title, p.markdown
               FROM pages p
               CROSS JOIN LATERAL page_capabilities(p.tenant_id, p.space_id, $4, p.id) access
              WHERE p.tenant_id = $1 AND (p.space_id, p.id) <> ($2, $3)
                AND EXISTS (SELECT 1 FROM page_permissions($1,$2,$4,$3) current_access
                    WHERE current_access.can_read AND current_access.can_browse_space)
                AND p.content_is_markdown AND access.can_read
                AND strpos(p.markdown, $5) > 0
              ORDER BY p.title_key, p.space_id, p.id
              LIMIT 100";
        let candidates: Vec<(Uuid, Uuid, Vec<u8>, String)> = sqlx::query_as(query)
            .bind(current.tenant)
            .bind(current.space)
            .bind(current.page)
            .bind(current.account)
            .bind(&target)
            .fetch_all(&self.0)
            .await?;
        candidates
            .into_iter()
            .map(|(space_id, id, title, markdown)| {
                Ok(BacklinkCandidate {
                    id,
                    space_id,
                    title: decode_title(title, "pages.title")?,
                    markdown,
                })
            })
            .collect()
    }
}

#[cfg(test)]
mod database_tests {
    use super::*;
    use crate::db::{CreatePage, CreatePageInput, CreateSection};

    async fn account(pool: &sqlx::PgPool, tenant: Uuid, email: &str) -> anyhow::Result<Uuid> {
        let id = Uuid::new_v4();
        sqlx::query(
            "INSERT INTO accounts (id,email,name,salt,password_hash) VALUES ($1,$2,$2,$3,$4)",
        )
        .bind(id)
        .bind(email)
        .bind(vec![0_u8; 16])
        .bind(vec![0_u8; 32])
        .execute(pool)
        .await?;
        sqlx::query("INSERT INTO tenant_memberships VALUES ($1,$2,'member','active',true,false)")
            .bind(tenant)
            .bind(id)
            .execute(pool)
            .await?;
        Ok(id)
    }

    #[sqlx::test(migrator = "crate::db::MIGRATIONS")]
    #[ignore = "requires isolated PostgreSQL with creation privileges"]
    async fn nested_documents_search_backlinks_and_revocation(
        pool: sqlx::PgPool,
    ) -> anyhow::Result<()> {
        let tenant = Uuid::new_v4();
        let foreign = Uuid::new_v4();
        sqlx::query("INSERT INTO tenants VALUES ($1,'Север'),($2,'Другая среда')")
            .bind(tenant)
            .bind(foreign)
            .execute(&pool)
            .await?;
        let owner = account(&pool, tenant, "owner@example.test").await?;
        let reader = account(&pool, tenant, "reader@example.test").await?;
        let other = account(&pool, foreign, "other@example.test").await?;
        let store = Store::from_test_pool(pool.clone());
        let space = Uuid::new_v4();
        sqlx::query("INSERT INTO spaces VALUES ($1,$2,$3,$3,'space','space')")
            .bind(tenant)
            .bind(space)
            .bind(owner)
            .execute(&pool)
            .await?;
        sqlx::query("INSERT INTO space_grants VALUES ($1,$2,$3,'viewer')")
            .bind(tenant)
            .bind(space)
            .bind(reader)
            .execute(&pool)
            .await?;
        let CreateSection::Saved(parent) = store
            .create_section(tenant, space, owner, Uuid::new_v4(), "План")
            .await?
        else {
            panic!("root section must be created")
        };
        let operation = Uuid::new_v4();
        let CreateSection::Saved(child) = store
            .create_section_under(tenant, space, owner, operation, "План", Some(parent))
            .await?
        else {
            panic!("may use the same section title beneath a different parent")
        };
        assert!(
            matches!(store.create_section_under(tenant,space,owner,operation,"План",Some(parent)).await?,CreateSection::Saved(id) if id == child)
        );
        assert!(matches!(
            store
                .create_section_under(tenant, space, owner, operation, "План", None)
                .await?,
            CreateSection::OperationConflict
        ));
        assert!(matches!(
            store
                .create_section_under(tenant, space, owner, Uuid::new_v4(), "План", Some(parent))
                .await?,
            CreateSection::Duplicate
        ));
        assert!(matches!(
            store
                .create_section_under(
                    tenant,
                    space,
                    owner,
                    Uuid::new_v4(),
                    "Другое",
                    Some(Uuid::new_v4())
                )
                .await?,
            CreateSection::MissingSpace
        ));
        let input = CreatePageInput {
            tenant_id: tenant,
            space_id: space,
            section_id: child,
            account_id: owner,
            operation_id: Uuid::new_v4(),
            title: "Заголовок поиска",
        };
        let CreatePage::Saved(root) = store.create_page(input).await? else {
            panic!("root page must be created")
        };
        let nested_input = CreatePageInput {
            operation_id: Uuid::new_v4(),
            title: "Дочерняя",
            ..input
        };
        let CreatePage::Saved(nested) = store.create_page_under(nested_input, Some(root)).await?
        else {
            panic!("child page must be created")
        };
        assert!(
            matches!(store.create_page_under(nested_input,Some(root)).await?,CreatePage::Saved(id) if id == nested)
        );
        assert!(matches!(
            store.create_page_under(nested_input, None).await?,
            CreatePage::OperationConflict
        ));
        assert!(matches!(
            store
                .create_page_under(
                    CreatePageInput {
                        section_id: parent,
                        operation_id: Uuid::new_v4(),
                        ..nested_input
                    },
                    Some(root)
                )
                .await?,
            CreatePage::MissingSection
        ));
        assert!(matches!(
            store
                .create_page_under(
                    CreatePageInput {
                        account_id: reader,
                        operation_id: Uuid::new_v4(),
                        ..nested_input
                    },
                    Some(root)
                )
                .await?,
            CreatePage::Forbidden
        ));

        let target = page_path(tenant, space, root);
        let source = format!("Частный поисковый текст\n\n[Родитель]({target})");
        sqlx::query("UPDATE pages SET markdown=$4, content_is_markdown=true WHERE (tenant_id,space_id,id)=($1,$2,$3)")
            .bind(tenant).bind(space).bind(nested).bind(source).execute(&pool).await?;
        let task = Uuid::new_v4();
        sqlx::query(
            "INSERT INTO workspace_tasks
                 (tenant_id,space_id,id,number,creator_account_id,operation_id,
                  request_fingerprint,title,status,source_page_id,created_at,updated_at)
             VALUES ($1,$2,$3,1,$4,$5,repeat('a',64),'Уточнение поиска','open',$6,0,0)",
        )
        .bind(tenant)
        .bind(space)
        .bind(task)
        .bind(owner)
        .bind(Uuid::new_v4())
        .bind(nested)
        .execute(&pool)
        .await?;
        let tree = store.sections(tenant, space, reader).await?.unwrap();
        assert_eq!(tree.sections[1].parent_id, Some(parent));
        assert_eq!(tree.pages[1].parent_id, Some(root));
        assert_eq!(tree.first_page().map(|page| page.id), Some(root));
        let results = store
            .search_documents(tenant, None, reader, "ПОИСКА", 1)
            .await?
            .unwrap();
        assert_eq!(results.items[0].id, root);
        assert!(results.items.iter().any(|item| item.id == task));
        let results = store
            .search_documents(tenant, Some(space), reader, "KB-001", 1)
            .await?
            .unwrap();
        assert_eq!(results.items.len(), 1);
        assert_eq!(results.items[0].id, task);
        assert_eq!(results.items[0].task_number, Some(1));
        let results = store
            .search_documents(tenant, Some(space), reader, "поисковый текст", 1)
            .await?
            .unwrap();
        assert_eq!(results.items[0].id, nested);
        assert!(results.items[0].snippet.contains("поисковый текст"));
        let page_matches = store
            .search_page_links(tenant, space, reader, "ПОИСКА", None, 1)
            .await?
            .unwrap();
        assert_eq!(page_matches.len(), 1);
        assert_eq!(page_matches[0].id, root);
        assert_eq!(page_matches[0].title, "Заголовок поиска");
        assert_eq!(page_matches[0].section_title, "План");
        assert!(
            store
                .search_page_links(tenant, space, reader, "поисковый текст", None, 1)
                .await?
                .unwrap()
                .is_empty()
        );
        assert!(
            store
                .search_page_links(tenant, space, other, "ПОИСКА", None, 1)
                .await?
                .is_none()
        );
        let selected = store
            .search_page_links(tenant, space, reader, "", Some(root), 1)
            .await?
            .unwrap();
        assert_eq!(selected.len(), 1);
        assert_eq!(selected[0].id, root);
        assert!(
            store
                .search_documents(tenant, None, other, "поисковый текст", 1)
                .await?
                .is_none()
        );
        let address = PageAddress {
            tenant,
            space,
            page: root,
            account: reader,
        };
        let other_space = Uuid::new_v4();
        sqlx::query("INSERT INTO spaces VALUES ($1,$2,$3,$3,'other','other')")
            .bind(tenant)
            .bind(other_space)
            .bind(owner)
            .execute(&pool)
            .await?;
        let CreateSection::Saved(other_section) = store
            .create_section(
                tenant,
                other_space,
                owner,
                Uuid::new_v4(),
                "Отдельный раздел",
            )
            .await?
        else {
            panic!("other section must be created")
        };
        let CreatePage::Saved(hidden_source) = store
            .create_page(CreatePageInput {
                space_id: other_space,
                section_id: other_section,
                operation_id: Uuid::new_v4(),
                title: "Закрытая исходная страница",
                ..input
            })
            .await?
        else {
            panic!("other page must be created")
        };
        sqlx::query("UPDATE pages SET markdown=$4,content_is_markdown=true WHERE (tenant_id,space_id,id)=($1,$2,$3)")
            .bind(tenant).bind(other_space).bind(hidden_source).bind(format!("[Родитель]({target})")).execute(&pool).await?;
        let links = store.backlink_candidates(&address).await?;
        assert_eq!(links.len(), 1);
        assert_eq!(links[0].id, nested);
        sqlx::query("INSERT INTO space_grants VALUES ($1,$2,$3,'viewer')")
            .bind(tenant)
            .bind(other_space)
            .bind(reader)
            .execute(&pool)
            .await?;
        let links = store.backlink_candidates(&address).await?;
        assert!(links.iter().any(|source| source.id == hidden_source));
        assert!(
            store
                .search_page_links(tenant, space, reader, "", Some(hidden_source), 1)
                .await?
                .unwrap()
                .is_empty()
        );
        sqlx::query("DELETE FROM space_grants WHERE (tenant_id,space_id,account_id)=($1,$2,$3)")
            .bind(tenant)
            .bind(space)
            .bind(reader)
            .execute(&pool)
            .await?;
        assert!(
            store
                .search_documents(tenant, None, reader, "поисковый текст", 1)
                .await?
                .unwrap()
                .items
                .is_empty()
        );
        assert!(
            store
                .search_documents(tenant, None, reader, "KB-001", 1)
                .await?
                .unwrap()
                .items
                .is_empty()
        );
        assert!(
            store
                .search_documents(tenant, Some(space), reader, "поисковый текст", 1)
                .await?
                .is_none()
        );
        assert!(store.backlink_candidates(&address).await?.is_empty());
        assert!(
            store
                .search_page_links(tenant, space, reader, "", Some(root), 1)
                .await?
                .is_none()
        );
        Ok(())
    }
}
