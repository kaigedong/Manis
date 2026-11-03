use gpui::Subscription;

#[derive(Default)]
pub(in crate::app) struct LifecycleSubscriptions {
    pub(in crate::app) window_bounds: Option<Subscription>,
    pub(in crate::app) app_lifecycle: Option<Subscription>,
}
