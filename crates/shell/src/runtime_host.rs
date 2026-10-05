//! Process services shared by the window and Android's headless Mail job.
//! Initialization must not reset approval/consent state when a window opens
//! after a job has already started the process.
use std::path::PathBuf;
use std::sync::Once;

pub fn init(data_dir: Option<String>, kernel: Option<PathBuf>) {
    static INIT: Once = Once::new();
    INIT.call_once(|| {
        let storage = crate::app_storage::init(data_dir.as_ref().map(PathBuf::from));
        #[cfg(any(feature = "app-hub", native_mobile))]
        octosense_app_hub_app::set_data_root(
            storage
                .map(|s| s.layout().apps_root().to_path_buf())
                .unwrap_or_else(|| {
                    data_dir
                        .as_ref()
                        .map(PathBuf::from)
                        .unwrap_or_else(crate::octosense::paths::home)
                        .join("apps")
                }),
        );
        let _ = storage;
        let mut host = crate::ai_host::Host::platform(data_dir.or_else(|| {
            Some(
                crate::octosense::paths::home()
                    .to_string_lossy()
                    .into_owned(),
            )
        }));
        if let Some(program) = kernel {
            host.kernel = crate::ai_host::KernelSource::Program(program);
        }
        crate::ai_host::start(host);
        let home = crate::octosense::paths::home();
        crate::dev_mode::init(&home);
        crate::octosense::paths::scope_linked_app_data();
        crate::approvals::init(&home);
        crate::host_tools::init();
        crate::system_chat::init(&home);
    });
}
