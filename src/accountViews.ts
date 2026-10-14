import { reactive } from 'vue';
import type { Chat, Connection, FieldName, Filter, ForwardReport, ScanResult, UnknownSend } from './types';

export function emptyView() {
  return {
    chatList: [] as Chat[], search: '', selectedChat: null as Chat | null,
    scopeKind: 'recent', rawCount: 50, fromDate: '', toDate: '', budget: 5000,
    filter: { kind: 'and', items: [] } as Filter, sortField: 'views' as FieldName,
    sortEmoji: '👍', descending: true, outputLimit: 20,
    result: null as ScanResult | null, currentRun: '', scanning: false, selected: [] as string[],
    target: '', confirmation: false, forwarding: false, report: null as ForwardReport | null,
    preview: null as ForwardReport | null, reportTarget: '',
    uncertainSends: [] as UnknownSend[],
    confirmedSelection: null as { target: string; keys: string[]; runId: string } | null,
    sendUpdates: {} as Record<string, string>, error: '', chatError: '', accountError: '', notice: '', busy: false,
  };
}
export type AccountView = ReturnType<typeof emptyView>;
export function createAccountViews() {
  const views = reactive<Record<string, AccountView>>({});
  const statuses = reactive<Record<string, Connection>>({});
  function get(id: string) { return views[id] ?? (views[id] = emptyView()); }
  function reset(id: string) { const uncertainSends=get(id).uncertainSends;views[id]={...emptyView(),uncertainSends}; }
  function connection(event: Connection) {
    const id = event.profile_id;
    if (!id || event.generation < (statuses[id]?.generation ?? 0)) return false;
    if (statuses[id] && event.generation !== statuses[id].generation) reset(id);
    statuses[id] = event;
    return true;
  }
  function progress(event: { profile_id: string; generation: number; result: ScanResult }) {
    const view = get(event.profile_id);
    if (event.generation !== statuses[event.profile_id]?.generation || event.result.run_id !== view.currentRun) return;
    view.result = event.result;
    view.scanning = event.result.status === 'Scanning';
    view.selected = view.selected.filter(key => event.result.posts.some(p => p.key === key));
  }
  function sent(event: {profile_id: string; generation: number; target: string; old_message_id: string; state: string}) {
    const view = get(event.profile_id);
    const same=(old:UnknownSend)=>old.generation===event.generation&&old.target===event.target&&old.old_message_id===event.old_message_id;
    if(event.state==='Unknown') { if(!view.uncertainSends.some(same))view.uncertainSends.push(event); }
    else if(event.state==='Sent'||event.state==='Failed')view.uncertainSends=view.uncertainSends.filter(old=>!same(old));
    if (event.generation !== statuses[event.profile_id]?.generation) return;
    view.sendUpdates[`${event.target}:${event.old_message_id}`] = event.state;
    if (event.target === view.reportTarget) for (const item of view.report?.items ?? []) {
      if (item.sent_id === event.old_message_id) item.state = event.state;
    }
  }
  return { views, statuses, get, reset, connection, progress, sent };
}
