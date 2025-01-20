use gtk::glib;

pub fn spawn_future_local_idle<R: 'static, F: std::future::Future<Output = R> + 'static>(
    f: F,
) -> glib::JoinHandle<R> {
    let ctx = glib::MainContext::ref_thread_default();
    ctx.spawn_local_with_priority(glib::Priority::DEFAULT_IDLE, f)
}
