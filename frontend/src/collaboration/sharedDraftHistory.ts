import type {createDraftHistory, DraftFrame} from '../editor/draftHistory';
import {applySplice, applySplices, carrySplice, draftSplice, selectionAfterSplice, type TextSplice} from './draftSplice';

export function rebaseSharedHistory<Frame extends DraftFrame>(
history: ReturnType<typeof createDraftHistory<Frame>>, displayed: string, remote: TextSplice[],
) {
  let backwardView=displayed;
  let backwardSteps=remote;
  history.walkBack(entry => {
    if (entry.after.source!==backwardView) return;
    const own=entry.span ?? draftSplice(entry.before.source,entry.after.source);
    const inverse={start:own.start,end:own.start+own.text.length,text:entry.before.source.slice(own.start,own.end)};
    const carried = carrySplice(inverse,backwardSteps);
    if (!carried) return;
    const after={...entry.after,source:applySplices(entry.after.source,backwardSteps),
      selection:backwardSteps.reduce(selectionAfterSplice,entry.after.selection)};
    const before={...entry.before,source:applySplice(after.source,carried.local),
      selection:carried.carried.reduce(selectionAfterSplice,entry.before.selection)};
    backwardView=entry.before.source;
    backwardSteps=carried.carried;
    return {before,after,span:{start:carried.local.start,end:carried.local.start+carried.local.text.length,text:own.text}};
  });
  history.walkForward(entry => {
    if (entry.before.source!==displayed) return;
    const own=entry.span ?? draftSplice(entry.before.source,entry.after.source);
    const carried=carrySplice(own,remote);
    if (!carried) return;
    const before={...entry.before,source:applySplices(entry.before.source,remote),
      selection:remote.reduce(selectionAfterSplice,entry.before.selection)};
    const after={...entry.after,source:applySplice(before.source,carried.local),
      selection:carried.carried.reduce(selectionAfterSplice,entry.after.selection)};
    displayed=entry.after.source;
    remote=carried.carried;
    return {before,after,span:carried.local};
  });
}
