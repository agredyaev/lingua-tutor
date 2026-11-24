//! Android JNI entry points for the native local runtime.

#[cfg(target_os = "android")]
mod android {
    use jni::{
        JNIEnv,
        objects::{JClass, JString},
        sys::{jint, jstring},
    };
    use lingua_tutor_local::{assets, server, storage};
    use std::{
        path::PathBuf,
        sync::atomic::{AtomicBool, AtomicU16, Ordering},
    };

    static RUNNING: AtomicBool = AtomicBool::new(false);
    static PORT: AtomicU16 = AtomicU16::new(0);

    fn publish_port(url: &str) {
        if let Some(port) = url
            .strip_prefix("http://127.0.0.1:")
            .and_then(|value| value.strip_suffix("/setup"))
            .and_then(|value| value.parse::<u16>().ok())
        {
            PORT.store(port, Ordering::Release);
        }
    }

    fn run(root: PathBuf, token: String) -> Result<(), String> {
        let state = root.join(".lingua-tutor");
        let db = state.join("lingua.sqlite3");
        let web = state.join("web");
        let existing = db.is_file();
        storage::open(&db)?;
        if existing {
            storage::backup_if_due(&db)?;
        }
        assets::write_to(&web)?;
        let env = storage::environment(&db)?;
        let runtime = tokio::runtime::Builder::new_current_thread()
            .enable_all()
            .build()
            .map_err(|error| error.to_string())?;
        runtime.block_on(server::serve_embedded(env, &web, token, publish_port))
    }

    // SAFETY: exact JNI name/signature for io.linguatutor.MainActivity.nativeRun.
    #[allow(unsafe_code)]
    #[unsafe(no_mangle)]
    pub extern "system" fn Java_io_linguatutor_MainActivity_nativeRun(
        mut env: JNIEnv<'_>,
        _class: JClass<'_>,
        directory: JString<'_>,
        token: JString<'_>,
    ) -> jstring {
        if RUNNING.swap(true, Ordering::AcqRel) {
            return std::ptr::null_mut();
        }
        let result = (|| {
            let directory: String = env
                .get_string(&directory)
                .map_err(|error| error.to_string())?
                .into();
            let token: String = env
                .get_string(&token)
                .map_err(|error| error.to_string())?
                .into();
            run(PathBuf::from(directory), token)
        })();
        PORT.store(0, Ordering::Release);
        RUNNING.store(false, Ordering::Release);
        match result {
            Ok(()) => std::ptr::null_mut(),
            Err(error) => env
                .new_string(error)
                .map_or(std::ptr::null_mut(), |message| message.into_raw()),
        }
    }

    // SAFETY: exact JNI name/signature for io.linguatutor.MainActivity.nativePort.
    #[allow(unsafe_code)]
    #[unsafe(no_mangle)]
    pub extern "system" fn Java_io_linguatutor_MainActivity_nativePort(
        _env: JNIEnv<'_>,
        _class: JClass<'_>,
    ) -> jint {
        jint::from(PORT.load(Ordering::Acquire))
    }
}
