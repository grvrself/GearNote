// 仅用于已选资料阅读的任务预算和停止信号。
use std::sync::{Arc, atomic::{AtomicBool, AtomicUsize, Ordering}};
use std::task::Waker;

struct ReadingControl { request_id: String, stopped: AtomicBool, waker: Mutex<Option<Waker>> }
impl ReadingControl {
    fn new(request_id: String) -> Self { Self { request_id, stopped: AtomicBool::new(false), waker: Mutex::new(None) } }
    fn stop(&self) {
        self.stopped.store(true, Ordering::SeqCst);
        if let Ok(waker) = self.waker.lock() { if let Some(waker) = waker.as_ref() { waker.wake_by_ref(); } }
    }
}

struct ReadingTask { control: Arc<ReadingControl>, started: Instant, budget: ReadingBudget, requests: AtomicUsize, calls: AtomicUsize }
impl ReadingTask {
    fn new(control: Arc<ReadingControl>, budget: ReadingBudget) -> Self {
        Self { control, started: Instant::now(), budget, requests: AtomicUsize::new(0), calls: AtomicUsize::new(0) }
    }
    fn reason(&self) -> Option<&'static str> {
        if self.control.stopped.load(Ordering::SeqCst) { Some("用户停止读取") }
        else if self.started.elapsed().as_secs() >= self.budget.deep_seconds { Some("达到整项阅读时间预算") }
        else { None }
    }
    fn stopped_by_user(&self) -> bool { self.control.stopped.load(Ordering::SeqCst) }
    /// 结构化阅读结束原因：用户停止与预算耗尽必须可区分。
    fn end_code(&self) -> ReadingEndReason {
        if self.stopped_by_user() { ReadingEndReason::UserStopped } else { ReadingEndReason::BudgetExhausted }
    }
    fn request_reason(&self) -> Option<&'static str> {
        self.reason().or_else(|| (self.requests.load(Ordering::SeqCst) >= self.budget.exploration_rounds).then_some("达到整项模型请求预算"))
    }
    fn progress(&self) -> String { format!("已发起 {} 次模型请求 · 已运行 {} 秒", self.requests.load(Ordering::SeqCst), self.started.elapsed().as_secs()) }
    fn remaining_seconds(&self) -> u64 { self.budget.deep_seconds.saturating_sub(self.started.elapsed().as_secs()).max(1).min(self.budget.request_seconds) }
    fn begin_request(&self) -> Result<(), String> {
        if let Some(reason) = self.request_reason() { return Err(reason.into()); }
        self.requests.fetch_add(1, Ordering::SeqCst); Ok(())
    }
    fn record_tool(&self) -> bool {
        if self.reason().is_some() || self.calls.load(Ordering::SeqCst) >= self.budget.tool_calls { return false; }
        self.calls.fetch_add(1, Ordering::SeqCst); true
    }
    async fn wait<T>(&self, future: impl std::future::Future<Output = Result<T, String>>) -> Result<T, String> {
        let mut future = Box::pin(future);
        std::future::poll_fn(|cx| {
            if let Ok(mut waker) = self.control.waker.lock() { *waker = Some(cx.waker().clone()); }
            if let Some(reason) = self.reason() { return std::task::Poll::Ready(Err(reason.into())); }
            future.as_mut().poll(cx)
        }).await
    }
    async fn json(&self, client: &reqwest::Client, config: &AgentConfig, key: &str, payload: serde_json::Value) -> Result<serde_json::Value, String> {
        self.begin_request()?;
        self.wait(async {
            let response = client.post(format!("{}/chat/completions", config.base_url)).bearer_auth(key)
                .timeout(Duration::from_secs(self.remaining_seconds())).json(&payload).send().await.map_err(|error| error.to_string())?;
            if !response.status().is_success() {
                let status = response.status(); let body = response.text().await.unwrap_or_default();
                return Err(provider_error(status, &body, &config.base_url, &config.model));
            }
            response.json().await.map_err(|error| format!("阅读响应格式无效：{error}"))
        }).await
    }
}

#[derive(Clone, Serialize)]
#[serde(rename_all = "camelCase")]
struct ReadingActivity { request_id: String, reading: bool, message: String }
struct ReadingLease<'a> { state: &'a AgentState, app: &'a AppHandle, control: Arc<ReadingControl> }
impl<'a> ReadingLease<'a> {
    fn start(state: &'a AgentState, app: &'a AppHandle, request_id: &str) -> Result<Self, String> {
        let control = Arc::new(ReadingControl::new(request_id.into()));
        *state.reading.lock().map_err(|_| "阅读状态暂不可用")? = Some(control.clone());
        let _ = app.emit("knowflow:reading-state", ReadingActivity { request_id: request_id.into(), reading: true, message: "正在自动阅读选中的资料…".into() });
        Ok(Self { state, app, control })
    }
}
impl Drop for ReadingLease<'_> {
    fn drop(&mut self) {
        if let Ok(mut active) = self.state.reading.lock() {
            if active.as_ref().is_some_and(|control| Arc::ptr_eq(control, &self.control)) { *active = None; }
        }
        let _ = self.app.emit("knowflow:reading-state", ReadingActivity { request_id: self.control.request_id.clone(), reading: false, message: "读取已结束，正在准备回答…".into() });
    }
}

#[tauri::command]
pub fn stop_agent_reading(state: State<'_, AgentState>, request_id: String) -> Result<(), String> {
    let active = state.reading.lock().map_err(|_| "阅读状态暂不可用")?;
    let control = active.as_ref().filter(|control| control.request_id == request_id).ok_or("本次读取已结束")?;
    control.stop();
    Ok(())
}
