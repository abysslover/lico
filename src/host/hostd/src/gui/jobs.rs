//! What a request to the host that does not block the window comes back as (`AsyncJob`) and its answer (`AsyncDone`): the window starts the request on a worker thread
//! (`App::spawn_host_call`) and handles the answer when the event loop gets it. Moved out of `panel.rs`, which has a size limit, without any change.

use serde_json::Value;

pub(crate) enum AsyncJob {
    FilesDir { req_id: u64, ssh_host: String, dir: String, keep_cursor: bool, fallback_tilde: bool, refreshed: bool },
    FilesPreview { req_id: u64, ssh_host: String, path: String, refreshed: bool },
    AccountCheck { req_id: u64, account: String },
    SessionStart { req_id: u64, created_cwd: Option<String> },
    SidebarDir { req_id: u64, ssh_host: Option<String>, dir: String, keep_page: bool, fallback_root: bool },
    SidebarNavSave,
    SidebarChats { req_id: u64, scope_key: String, page: usize },
    ChatResume { req_id: u64, created_cwd: Option<String> }, NewChatLast { req_id: u64, account: String }, NewChatStart { req_id: u64, created_cwd: Option<String>, resume: bool },
    CallGet { call_id: String }, CallSay { call_id: String, text: String },
    CallStart, CallAction,
    LlmFind { req_id: u64 },
}

pub(crate) struct AsyncDone {
    pub job: AsyncJob,
    pub reply: Value,
}
