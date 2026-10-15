export interface MessageKey { chat_id: string; message_id: string }
export interface Reaction { key: { kind: 'emoji' | 'custom_emoji' | 'paid' | 'unknown'; value?: string }; count: number }
export interface Metrics { views: number | null; forwards: number | null; reply_info: number | null; reactions: Reaction[] | null }
export interface Message { key: MessageKey; date: number; content_kind: string; text: string; media_album_id: string | null; metrics: Metrics }
export interface Post { key: string; kind: 'single' | 'album'; members: Message[]; representative: MessageKey; date: number; text: string; metrics: Metrics; provenance: Record<string, string>; partial: boolean }
export interface ScanResult { run_id: string; status: 'Scanning' | 'Partial' | 'Complete' | 'Cancelled' | 'Error'; scanned: number; pages: number; exact: boolean; detail: string; posts: Post[] }
export interface Chat { id: string; title: string; kind: string; archived?: boolean; photo_file_id?: number|null }
export type ChatScope = 'all' | 'main' | 'archive';
export interface Connection { profile_id: string | null; generation: number; runtime_state: string; auth_state: string; connection_state: string; detail: string; user_disconnected: boolean }
export type FieldName = 'views' | 'forwards' | 'comments' | 'reply_info' | 'reaction_total' | 'reaction' | 'post_kind' | 'media_album_id' | 'content_kind' | 'date';
export interface Field { kind: FieldName; emoji?: string }
export type CompareOp = 'eq' | 'ne' | 'gt' | 'ge' | 'lt' | 'le';
export type Filter = { kind: 'compare'; field: Field; op: CompareOp; value: number | string } | { kind: 'is_null' | 'is_not_null'; field: Field } | { kind: 'and' | 'or'; items: Filter[] } | { kind: 'not'; item: Filter };
export interface QuerySpec { version: 1; chat_id: string; scope: { kind: 'recent'; count: number } | { kind: 'dates'; from: number; to: number; budget: number }; filter: Filter; sort: { field: Field; descending: boolean }[]; output_limit: number }
export interface ActionItem { source: MessageKey; state: string; detail: string; sent_id: string | null }
export interface ForwardReport { items: ActionItem[] }
export interface UnknownSend { profile_id: string; generation: number; target: string; old_message_id: string; state: string }
export interface CloseChallenge { pending_count: number; retry_after_ms: number; token: string | null }
export interface Settings { schema_version: number; api_id: number | null; proxy_enabled: boolean; proxy_server: string; proxy_port: number; proxy_username: string }
export interface Profile { profile_id: string; telegram_user_id: string | null; display_name: string; username: string | null; is_legacy_profile: boolean; session_directory_id: string; created_at: number; last_used_at: number; has_saved_database_secret: boolean; logout_pending: boolean; logout_completed?: boolean }
export interface DebugResetReport { completed:boolean; accounts:Array<{profile_id:string;display_name:string;completed:boolean;error:string|null}>;error:string|null }
export interface SettingsSnapshot { settings: Settings; profiles: Profile[]; has_saved_api_hash: boolean; has_saved_proxy_password: boolean; primary_profile_id: string | null; interrupted_profiles: Profile[]; debug_tools: boolean }
