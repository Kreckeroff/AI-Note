use serde::Serialize;

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize)]
#[serde(rename_all = "lowercase")]
pub(crate) enum PageContentFormat {
    Literal,
    Markdown,
}

impl PageContentFormat {
    pub(crate) fn as_str(self) -> &'static str {
        match self {
            Self::Literal => "literal",
            Self::Markdown => "markdown",
        }
    }

    pub(super) fn from_markdown(is_markdown: bool) -> Self {
        if is_markdown {
            Self::Markdown
        } else {
            Self::Literal
        }
    }
}
