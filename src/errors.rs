use jni::JNIEnv;
use thiserror::Error;

#[derive(Debug, Error)]
pub enum JNIError {
    #[error("Java exception thrown: {class}: {message}")]
    JavaException { class: String, message: String },
    #[error(transparent)]
    Jni(#[from] jni::errors::Error),
}

/// Converts a raw `jni` error into a `JNIError`.
pub fn from_jni_error(env: &mut JNIEnv, err: jni::errors::Error) -> JNIError {
    if matches!(err, jni::errors::Error::JavaException) {
        if let Some((class, message)) = describe_pending_exception(env) {
            return JNIError::JavaException { class, message };
        }
    }
    JNIError::Jni(err)
}

fn describe_pending_exception(env: &mut JNIEnv) -> Option<(String, String)> {
    let throwable = env.exception_occurred().ok()?;
    if throwable.as_raw().is_null() {
        return None;
    }
    
    let _ = env.exception_clear();

    let class = env
        .get_object_class(&throwable)
        .and_then(|c| env.call_method(c, "getName", "()Ljava/lang/String;", &[]))
        .and_then(|v| v.l())
        .and_then(|o| env.get_string((&o).into()).map(String::from))
        .unwrap_or_else(|_| "<unknown exception class>".to_string());

    let message = env
        .call_method(&throwable, "getMessage", "()Ljava/lang/String;", &[])
        .and_then(|v| v.l())
        .ok()
        .filter(|o| !o.as_raw().is_null())
        .and_then(|o| env.get_string((&o).into()).ok().map(String::from))
        .unwrap_or_else(|| "<no message>".to_string());

    Some((class, message))
}
