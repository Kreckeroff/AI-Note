import type {SourceRange, TextRun} from './markdownModel';

export type SlashMatch = {query: string; range: SourceRange};

export function findSlashCommand(runs: TextRun[], caret: number): SlashMatch | undefined {
  let preceding = '';
  for (const run of runs) {
    const index = run.offsets.lastIndexOf(caret);
    if (index > 0 && !run.locked && !run.marks.some(mark => mark.kind === 'code' || mark.kind === 'link')) {
      const prefix = preceding + run.text.slice(0, index);
      const token = /(?:^|\s)(\/[^/\s]{0,28})$/u.exec(prefix)?.[1];
      if (token && index >= token.length) {
        return {query: token.slice(1).toLocaleLowerCase('ru'), range: {start: run.offsets[index - token.length], end: caret}};
      }
    }
    if (run.text) preceding = run.text.slice(-1);
  }
}
