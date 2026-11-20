import {newOperationId} from './api';
import {
  openDraft, readDraft, writeDraft, type DraftCommand, type DraftConflict, type DraftReply, type DraftSnapshot,
} from './coeditingApi';
import {
  applySplice, applySplices, carrySplice, draftSplice, orderedSplices, sameDraftText, scalarOffset, sourceOffset,
  type DraftText, type TextSplice,
} from './draftSplice';

type Edit = {
  before: DraftText;
  after: DraftText;
  span: TextSplice;
  id: string;
  command?: DraftCommand;
};
type Resolution = Edit & {rejectedId?: string};

type FragmentConflict = {
  start: number; end: number; mine: string; theirs: string;
  snapshot: DraftSnapshot;
  persistent: boolean;
  rejectedId?: string;
};

export type SharedDraftView = {
  status: 'opening' | 'ready' | 'sending' | 'offline' | 'conflict' | 'expired' | 'recover' | 'rejected';
  snapshot?: DraftSnapshot;
  pending: number;
  canResolve: boolean;
  conflict?: FragmentConflict;
  mergeInput?: string;
  recovery?: string;
  copy?: string;
  message: string;
};

type DraftHandlers = {
  onDraft(text: DraftText, steps?: TextSplice[]): void;
  onState(view: SharedDraftView): void;
  onDenied(): void;
};

type Attempt = {kind: 'open' | 'read'} | {kind: 'write'; edit: Edit};
const pollMs = 1000;
const editDelayMs = 65;
const retryMs = 1500;

export class SharedDraftSession {
  private snapshot?: DraftSnapshot;
  private queue: Edit[] = [];
  private transient: Edit[] = [];
  private mergeInput?: string;
  private resolution?: Resolution;
  private paused = false;
  private request?: AbortController;
  private working = false;
  private scheduled?: number;
  private deferred?: {attempt: Attempt; reply: DraftReply};
  private conflict?: FragmentConflict;
  private status: SharedDraftView['status'] = 'opening';
  private message = '';
  private held?: DraftText;
  private copy?: string;

  constructor(private readonly path: string, private readonly handlers: DraftHandlers, preserved?: DraftText) {
    this.held = preserved;
  }

  start() {
    this.request = new AbortController();
    this.schedule(0);
    window.addEventListener('online', this.wake);
  }

  stop() {
    this.request?.abort();
    this.request = undefined;
    if (this.scheduled !== undefined) window.clearTimeout(this.scheduled);
    this.scheduled = undefined;
    this.deferred = undefined;
    this.working = false;
    window.removeEventListener('online', this.wake);
  }

  private wake = () => this.schedule(0);

  setPaused(value: boolean) {
    if (this.paused === value) return;
    this.paused = value;
    if (!value) this.schedule(0);
  }

  edit(next: DraftText, authoredSpan?: TextSplice) {
    if (!this.snapshot || this.resolution || this.held && !this.conflict) return false;
    // A transient conflict already rejected one specific edit. Keep new
    // disjoint typing visible, but separate from that rejected operation.
    const transient=this.conflict && !this.conflict.persistent;
    const edits=transient ? this.transient : this.queue;
    const before = edits.at(-1)?.after ?? this.snapshot;
    if (sameDraftText(before,next)) return true;
    const span = authoredSpan && applySplice(before.source,authoredSpan)===next.source ?
      authoredSpan : draftSplice(before.source,next.source);
    if (this.conflict) {
      let start=this.conflict.start;
      let end=this.conflict.end;
      for (const prior of edits) {
        if (prior.span.end<=start) {
          const shift=prior.span.text.length-prior.span.end+prior.span.start;
          start+=shift;
          end+=shift;
        }
      }
      const identicalGap=start===end && span.start===start && span.end===end;
      if (identicalGap || span.start<end && span.end>start || next.format!==before.format) return false;
    }
    const edit: Edit = {before,after:next,span,id:newOperationId()};
    // This command is authored against the snapshot visible when typing began,
    // even if a previously issued poll returns before we send the command.
    if (!transient && !edits.length) edit.command=this.command(this.snapshot,edit);
    edits.push(edit);
    this.emit();
    this.schedule(editDelayMs);
    return true;
  }

  reopen() {
    if (this.status !== 'expired' && this.status !== 'rejected') return;
    this.holdQueue();
    this.holdTransient();
    this.preserveMergeInput();
    this.snapshot = undefined;
    this.conflict = undefined;
    this.setState('opening');
    this.schedule(0);
  }

  useCurrent() {
    if (this.status !== 'recover' || !this.snapshot) return;
    if (this.held && !sameDraftText(this.held,this.snapshot)) this.keepCopy(this.held.source);
    this.held = undefined;
    this.handlers.onDraft(this.snapshot);
    this.setState('ready');
    this.schedule(0);
  }

  dismissCopy() {
    this.copy = undefined;
    this.emit();
  }

  changeMergeInput(text: string) {
    if (!this.conflict || this.status!=='conflict') return;
    this.mergeInput=text===this.conflict.mine ? undefined : text;
    this.emit();
  }

  private preserveMergeInput() {
    if (this.mergeInput!==undefined) this.keepCopy(this.mergeInput);
    this.mergeInput=undefined;
  }

  private holdTransient() {
    const intended=this.transient.at(-1)?.after;
    if (!intended) return;
    if (!this.held) this.held=intended;
    else if (!sameDraftText(this.held,intended)) this.keepCopy(intended.source);
    this.transient=[];
  }

  resolve(text: string) {
    const conflict = this.conflict;
    if (!conflict || this.status !== 'conflict' || this.queue.length && !conflict.rejectedId && !conflict.persistent) return;
    if (conflict.persistent && this.queue.length) return;
    const span = {start:conflict.start,end:conflict.end,text};
    const chosen = {source:applySplice(conflict.snapshot.source,span),format:conflict.snapshot.format};
    const resolution: Resolution = {
      before:conflict.snapshot,after:chosen,span,id:newOperationId(),rejectedId:conflict.rejectedId,
    };
    resolution.command = {...this.command(conflict.snapshot,resolution),resolve:true};
    this.resolution = resolution;
    this.mergeInput = undefined;
    this.snapshot = conflict.snapshot;
    this.conflict = undefined;
    this.setState('sending');
    this.schedule(0);
  }

  hasUnconfirmed(): boolean {
    return this.queue.length>0 || this.transient.length>0 || this.mergeInput!==undefined ||
      (this.copy!==undefined || Boolean(this.held || this.resolution));
  }

  private command(snapshot: DraftSnapshot, edit: Edit): DraftCommand {
    return {
      action: 'apply', lease: snapshot.lease, operationId: edit.id, version: snapshot.version,
      start: scalarOffset(edit.before.source,edit.span.start), end: scalarOffset(edit.before.source,edit.span.end),
      text: edit.span.text, format: edit.after.format,
    };
  }

  private schedule(delay: number) {
    if (!this.request || this.working || this.paused || ['rejected','expired','recover'].includes(this.status) ||
        this.status==='conflict' && !this.conflict?.persistent) return;
    if (this.scheduled !== undefined) window.clearTimeout(this.scheduled);
    this.scheduled = window.setTimeout(() => {
      this.scheduled = undefined;
      void this.tick();
    }, delay);
  }

  private async tick() {
    if (this.paused || this.working || !this.request) return;
    if (this.deferred) {
      const {attempt, reply} = this.deferred;
      this.deferred = undefined;
      this.receive(attempt, reply);
      return;
    }
    const request = this.request;
    this.working = true;
    let attempt: Attempt;
    let reply: DraftReply;
    const head = this.resolution ?? this.queue[0];
    if (!this.snapshot) {
      attempt = {kind:'open'};
      reply = await openDraft(this.path,request.signal);
    } else if (head) {
      attempt = {kind:'write',edit:head};
      if (!head.command && !sameDraftText(head.before,this.snapshot)) {
        this.working=false;
        this.holdQueue();
        this.showSnapshot(this.snapshot);
        return;
      }
      head.command ??= this.command(this.snapshot,head);
      if (this.status!=='offline' && !this.conflict?.persistent) this.setState('sending');
      reply = await writeDraft(this.path,head.command,request.signal);
    } else {
      attempt = {kind:'read'};
      reply = await readDraft(this.path,this.snapshot,request.signal);
    }
    if (request !== this.request || request.signal.aborted) return;
    this.working = false;
    if (this.paused && reply.kind !== 'denied') {
      this.deferred = {attempt,reply};
      return;
    }
    this.receive(attempt,reply);
  }

  private receive(attempt: Attempt, reply: DraftReply) {
    switch (reply.kind) {
      case 'denied':
        this.stop();
        this.snapshot = undefined;
        this.queue = [];
        this.transient = [];
        this.mergeInput = undefined;
        this.held = undefined;
        this.copy = undefined;
        this.resolution = undefined;
        this.conflict = undefined;
        this.handlers.onDenied();
        return;
      case 'retry':
        this.setState('offline');
        this.schedule(retryMs);
        return;
      case 'expired':
        this.setState('expired');
        return;
      case 'baseExpired':
        if (attempt.kind==='write' && reply.snapshot.acknowledgedVersion) this.consume(attempt.edit);
        this.holdQueue();
        this.holdTransient();
        this.preserveMergeInput();
        this.snapshot=reply.snapshot;
        this.conflict=undefined;
        this.showSnapshot(reply.snapshot);
        return;
      case 'rejected':
        this.setState('rejected',reply.message);
        return;
      case 'conflict':
        this.receiveConflict(attempt,reply.conflict);
        return;
      case 'snapshot':
        this.receiveSnapshot(attempt,reply.snapshot);
        return;
      case 'unchanged':
        this.setState(this.conflict ? 'conflict' : 'ready');
        this.schedule(this.queue.length ? 0 : pollMs);
    }
  }

  private receiveConflict(attempt: Attempt, conflict: DraftConflict) {
    if (attempt.kind==='read' && this.conflict?.persistent && conflict.persistent && this.queue.length) {
      this.setState('conflict');
      this.schedule(0);
      return;
    }
    const previous=this.snapshot;
    let displayed: DraftText=conflict;
    let steps=previous ? this.stepsFrom(conflict,previous,previous.version) : undefined;
    if (attempt.kind==='write' && conflict.acknowledgedVersion) {
      const edit=attempt.edit;
      this.consume(edit);
      const rebased=this.rebaseTail(edit,conflict);
      displayed=rebased.text;
      steps=rebased.steps;
    } else if (conflict.persistent && this.queue.length) {
      // The server retained the shared conflict. A queued command that it
      // did not acknowledge is preserved separately from the common draft.
      this.holdQueue();
    }
    let rejectedId: string | undefined;
    if (attempt.kind==='write' && !conflict.persistent && !conflict.acknowledgedVersion) {
      if (attempt.edit===this.resolution) rejectedId=this.resolution?.rejectedId;
      else if (attempt.edit===this.queue[0]) rejectedId=attempt.edit.id;
    }
    if (attempt.kind==='write' && attempt.edit===this.resolution) {
      this.keepCopy(this.resolution!.after.source);
      this.resolution=undefined;
    }
    if (!this.conflict || this.conflict.mine!==conflict.mine || this.conflict.theirs!==conflict.theirs) {
      this.preserveMergeInput();
    }
    if (this.transient.length) this.holdTransient();
    this.snapshot=conflict;
    this.conflict={
      start:sourceOffset(conflict.source,conflict.start),end:sourceOffset(conflict.source,conflict.end),
      mine:conflict.mine,theirs:conflict.theirs,snapshot:conflict,persistent:conflict.persistent,rejectedId,
    };
    this.showConflict(displayed,steps);
  }

  private showConflict(displayed:DraftText, steps?:TextSplice[]) {
    this.handlers.onDraft(displayed,steps);
    this.setState('conflict');
    if (this.conflict?.persistent) this.schedule(this.queue.length ? 0 : pollMs);
  }

  private receiveSnapshot(attempt: Attempt, snapshot: DraftSnapshot) {
    const previous=this.snapshot;
    const previousConflict=this.conflict;
    const retained=snapshot.restoreConflict;
    if (!retained || previousConflict?.mine!==retained.mine || previousConflict?.theirs!==retained.theirs) {
      this.preserveMergeInput();
    }
    this.snapshot=snapshot;
    this.conflict=undefined;
    if (attempt.kind==='write') {
      const edit=attempt.edit;
      const resolution=edit===this.resolution;
      this.consume(edit);
      if (resolution) {
        // Older commands still depend on the rejected edit. New disjoint
        // input was authored against the displayed conflict snapshot and
        // can be carried through the confirmed resolution.
        if (this.queue.length) this.holdQueue();
        this.queue=this.transient;
        this.transient=[];
        const steps=edit.command && this.stepsFrom(snapshot,edit.before,edit.command.version);
        const rebased=this.rebaseQueue(snapshot,edit.before,steps);
        this.showSnapshot(rebased.text,rebased.steps);
        return;
      }
      const rebased=this.rebaseTail(edit,snapshot);
      if (retained) {
        this.conflict={
          ...retained,start:sourceOffset(snapshot.source,retained.start),end:sourceOffset(snapshot.source,retained.end),
          snapshot,persistent:true,
        };
        this.showConflict(rebased.text,rebased.steps);
      } else this.showSnapshot(rebased.text,rebased.steps);
      return;
    }
    // The queued head keeps its authored command. The next server ACK
    // supplies exact transformations for the remaining optimistic edits.
    if (this.queue.length) {
      this.setState('ready');
      this.schedule(0);
      return;
    }
    const steps = previous && this.stepsFrom(snapshot,previous,previous.version);
    this.showSnapshot(snapshot,steps);
  }

  private consume(edit: Edit) {
    if (edit===this.resolution) {
      if (this.resolution.rejectedId && this.queue[0]?.id===this.resolution.rejectedId) this.queue.shift();
      this.resolution=undefined;
    } else if (edit===this.queue[0]) this.queue.shift();
  }

  private stepsFrom(snapshot: DraftSnapshot, before: DraftText, version: string): TextSplice[] | undefined {
    if (snapshot.fromVersion===version && snapshot.steps) {
      const history = orderedSplices(before.source,snapshot.steps);
      if (history?.source===snapshot.source) return history.splices;
    }
    return sameDraftText(before,snapshot) ? [] : undefined;
  }

  private rebaseTail(head: Edit, snapshot: DraftSnapshot) {
    let remote: TextSplice[] | undefined;
    const recorded=head.command && snapshot.fromVersion===head.command.version && snapshot.steps &&
      this.stepsFrom(snapshot,head.before,head.command.version);
    if (recorded) {
      const carried=carrySplice(head.span,snapshot.applied ? recorded.slice(0,-1) : recorded);
      if (carried && applySplices(head.after.source,carried.carried)===snapshot.source) remote=carried.carried;
    } else if (sameDraftText(head.after,snapshot)) remote=[];
    // The exact journal already describes the change to the optimistic
    // history; sending a separate acknowledgement would move the same entry twice.
    return this.rebaseQueue(snapshot,head.after,remote);
  }

  private rebaseQueue(snapshot: DraftSnapshot, base: DraftText, remote?: TextSplice[]) {
    let staged: DraftText=snapshot;
    let formatSafe=base.format===snapshot.format;
    for (let index=0;index<this.queue.length;index++) {
      const edit=this.queue[index];
      const converted=remote && formatSafe && carrySplice(edit.span,remote);
      if (!converted) {
        this.holdQueue(index);
        break;
      }
      const next={source:applySplice(staged.source,converted.local),format:edit.after.format};
      this.queue[index]={...edit,before:staged,after:next,span:converted.local,command:undefined};
      staged=next;
      remote=converted.carried;
      formatSafe=true;
    }
    return {text:staged,steps:remote};
  }

  private showSnapshot(text: DraftText, steps?: TextSplice[]) {
    if (this.held && !this.queue.length) {
      if (this.snapshot && sameDraftText(this.held,this.snapshot)) this.held=undefined;
      else {
        this.handlers.onDraft(text,steps);
        this.setState('recover');
        return;
      }
    }
    this.handlers.onDraft(text,steps);
    this.setState('ready');
    this.schedule(this.queue.length ? 0 : pollMs);
  }

  private keepCopy(source: string) {
    if (this.copy===undefined) this.copy=source;
    else if (this.copy!==source) this.copy+='\n\n'+source;
  }

  private holdQueue(from=0) {
    const pending=this.queue.slice(from);
    if (pending.length) {
      const intended=pending.at(-1)!.after;
      if (!this.held) this.held=intended;
      else if (!sameDraftText(this.held,intended)) this.keepCopy(intended.source);
    }
    this.queue=this.queue.slice(0,from);
    if (this.resolution) {
      this.keepCopy(this.resolution.after.source);
      this.resolution=undefined;
    }
  }

  private setState(status: SharedDraftView['status'], message = '') {
    this.status = status;
    this.message = message;
    this.emit();
  }

  private emit() {
    this.handlers.onState({
      status:this.status,snapshot:this.snapshot,
      pending:this.queue.length+this.transient.length+Number(Boolean(this.held))+Number(Boolean(this.resolution)),
      canResolve:!this.conflict?.persistent || this.queue.length===0,
      conflict:this.conflict,mergeInput:this.mergeInput,recovery:this.held?.source,copy:this.copy,message:this.message,
    });
  }
}
