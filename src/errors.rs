use thiserror::Error;

/// Error returned by generated JNI bindings.
///
/// Wraps the underlying `jni` crate error (missing class, missing method,
/// a thrown Java exception, etc.) instead of discarding it.
#[derive(Debug, Error)]
#[error(transparent)]
pub struct JNIError(#[from] pub jni::errors::Error);
