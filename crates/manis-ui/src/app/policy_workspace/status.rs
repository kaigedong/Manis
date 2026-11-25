use super::{ControllerState, LiveStreamPhase, ManisApp, Message, copy};

impl ManisApp {
    /// The live-stream problem that outranks the ordinary status line, if there is one.
    ///
    /// The rail telemetry card and the window title both read this, so a stalled activity or log
    /// stream is never hidden behind a healthy-looking connection.
    pub(in crate::app) fn live_status_issue(&self) -> Option<String> {
        if !matches!(self.controller, ControllerState::Connected { .. }) {
            return None;
        }
        let language = self.language();
        let is_issue = |phase: &LiveStreamPhase| {
            !matches!(
                phase,
                LiveStreamPhase::Waiting | LiveStreamPhase::Connecting | LiveStreamPhase::Live
            )
        };
        if self.live_status.activity == self.live_status.logs
            && is_issue(&self.live_status.activity)
        {
            return Some(copy::app::live_stream_phase(
                language,
                &self.live_status.activity,
            ));
        }
        let issues = [
            (Message::NetworkActivity, &self.live_status.activity),
            (Message::Logs, &self.live_status.logs),
        ]
        .into_iter()
        .filter(|(_, phase)| is_issue(phase))
        .map(|(source, phase)| {
            format!(
                "{}：{}",
                language.message(source),
                copy::app::live_stream_phase(language, phase)
            )
        })
        .collect::<Vec<_>>();
        (!issues.is_empty()).then(|| issues.join(" · "))
    }
}
