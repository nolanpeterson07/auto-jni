/// Call a static Java method, caching the method ID in a `OnceCell`.
/// Expands to a `Result<JValueOwned, auto_jni::errors::JNIError>`.
#[macro_export]
macro_rules! call_static {
    ($path:tt, $method:tt, $sig:tt, $args:expr, $ret:expr) => {{
        use auto_jni::once_cell::sync::OnceCell;
        use auto_jni::jni::objects::{JClass, JStaticMethodID};
        use crate::java;
        static FNPTR: OnceCell<JStaticMethodID> = OnceCell::new();
        static CLASS: OnceCell<JClass> = OnceCell::new();
        let mut env = java();
        (|| -> auto_jni::jni::errors::Result<_> {
            let fnptr = FNPTR.get_or_try_init(|| env.get_static_method_id($path, $method, $sig))?;
            let class = CLASS.get_or_try_init(|| env.find_class($path))?;
            unsafe { env.call_static_method_unchecked(class, fnptr, $ret, $args) }
        })()
        .map_err(|e| auto_jni::errors::from_jni_error(&mut env, e))
    }};
}

/// Call an instance Java method, caching the method ID in a `OnceCell`.
/// Expands to a `Result<JValueOwned, auto_jni::errors::JNIError>`.
#[macro_export]
macro_rules! call {
    ($obj:expr, $path:tt, $method:tt, $sig:tt, $args:expr, $ret:expr) => {{
        use auto_jni::once_cell::sync::OnceCell;
        use auto_jni::jni::objects::JMethodID;
        use crate::java;
        static FNPTR: OnceCell<JMethodID> = OnceCell::new();
        let mut env = java();
        (|| -> auto_jni::jni::errors::Result<_> {
            let fnptr = FNPTR.get_or_try_init(|| -> auto_jni::jni::errors::Result<JMethodID> {
                let class = env.find_class($path)?;
                env.get_method_id(class, $method, $sig)
            })?;
            unsafe { env.call_method_unchecked($obj, fnptr, $ret, $args) }
        })()
        .map_err(|e| auto_jni::errors::from_jni_error(&mut env, e))
    }};
}

/// Construct a Java object, caching the constructor ID in a `OnceCell`.
/// Expands to a `Result<GlobalRef, auto_jni::errors::JNIError>`.
#[macro_export]
macro_rules! create {
    ($path:tt, $sig:tt, $args:expr) => {{
        use auto_jni::once_cell::sync::OnceCell;
        use auto_jni::jni::objects::{JClass, JMethodID};
        use crate::java;
        static FNPTR: OnceCell<JMethodID> = OnceCell::new();
        static CLASS: OnceCell<JClass> = OnceCell::new();
        let mut env = java();
        (|| -> auto_jni::jni::errors::Result<_> {
            let class = CLASS.get_or_try_init(|| env.find_class($path))?;
            let fnptr = FNPTR.get_or_try_init(|| env.get_method_id(class, "<init>", $sig))?;
            let obj = unsafe { env.new_object_unchecked(class, *fnptr, $args) }?;
            env.new_global_ref(obj)
        })()
        .map_err(|e| auto_jni::errors::from_jni_error(&mut env, e))
    }};
}

/// Read an instance field, caching the field ID in a `OnceCell`.
/// Expands to a `Result<JValueOwned, auto_jni::errors::JNIError>`.
#[macro_export]
macro_rules! get_field {
    ($obj:expr, $path:tt, $name:tt, $sig:tt) => {{
        use auto_jni::once_cell::sync::OnceCell;
        use auto_jni::jni::objects::JFieldID;
        use auto_jni::jni::signature::ReturnType;
        use std::str::FromStr;
        use crate::java;
        static FIELD: OnceCell<JFieldID> = OnceCell::new();
        let mut env = java();
        (|| -> auto_jni::jni::errors::Result<_> {
            let field = FIELD.get_or_try_init(|| -> auto_jni::jni::errors::Result<JFieldID> {
                let class = env.find_class($path)?;
                env.get_field_id(class, $name, $sig)
            })?;

            env.get_field_unchecked($obj, field, ReturnType::from_str($sig).unwrap())
        })()
        .map_err(|e| auto_jni::errors::from_jni_error(&mut env, e))
    }};
}

/// Write an instance field, caching the field ID in a `OnceCell`.
/// Expands to a `Result<(), auto_jni::errors::JNIError>`.
#[macro_export]
macro_rules! set_field {
    ($obj:expr, $path:tt, $name:tt, $sig:tt, $val:expr) => {{
        use auto_jni::once_cell::sync::OnceCell;
        use auto_jni::jni::objects::JFieldID;
        use crate::java;
        static FIELD: OnceCell<JFieldID> = OnceCell::new();
        let mut env = java();
        (|| -> auto_jni::jni::errors::Result<_> {
            let field = FIELD.get_or_try_init(|| -> auto_jni::jni::errors::Result<JFieldID> {
                let class = env.find_class($path)?;
                env.get_field_id(class, $name, $sig)
            })?;
            env.set_field_unchecked($obj, field, $val)
        })()
        .map_err(|e| auto_jni::errors::from_jni_error(&mut env, e))
    }};
}

/// Read a static field, caching the field ID in a `OnceCell`.
/// Expands to a `Result<JValueOwned, auto_jni::errors::JNIError>`.
#[macro_export]
macro_rules! get_static_field {
    ($path:tt, $name:tt, $sig:tt) => {{
        use auto_jni::once_cell::sync::OnceCell;
        use auto_jni::jni::objects::{JClass, JStaticFieldID};
        use auto_jni::jni::signature::JavaType;
        use std::str::FromStr;
        use crate::java;
        static FIELD: OnceCell<JStaticFieldID> = OnceCell::new();
        static CLASS: OnceCell<JClass> = OnceCell::new();
        let mut env = java();
        (|| -> auto_jni::jni::errors::Result<_> {
            let field = FIELD.get_or_try_init(|| env.get_static_field_id($path, $name, $sig))?;
            let class = CLASS.get_or_try_init(|| env.find_class($path))?;

            env.get_static_field_unchecked(class, field, JavaType::from_str($sig).unwrap())
        })()
        .map_err(|e| auto_jni::errors::from_jni_error(&mut env, e))
    }};
}

/// Write a static field, caching the field ID in a `OnceCell`.
/// Expands to a `Result<(), auto_jni::errors::JNIError>`.
#[macro_export]
macro_rules! set_static_field {
    ($path:tt, $name:tt, $sig:tt, $val:expr) => {{
        use auto_jni::once_cell::sync::OnceCell;
        use auto_jni::jni::objects::{JClass, JStaticFieldID};
        use crate::java;
        static FIELD: OnceCell<JStaticFieldID> = OnceCell::new();
        static CLASS: OnceCell<JClass> = OnceCell::new();
        let mut env = java();
        (|| -> auto_jni::jni::errors::Result<_> {
            let field = FIELD.get_or_try_init(|| env.get_static_field_id($path, $name, $sig))?;
            let class = CLASS.get_or_try_init(|| env.find_class($path))?;
            env.set_static_field(class, field, $val)
        })()
        .map_err(|e| auto_jni::errors::from_jni_error(&mut env, e))
    }};
}
