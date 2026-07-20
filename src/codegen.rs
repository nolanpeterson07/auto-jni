use std::collections::HashMap;
use std::fs::File;
use std::io::Write;
use std::path::Path;

use jni::signature::{Primitive, ReturnType};

use crate::parse_javap_output;

/// Generate a Rust source file with JNI bindings for the given Java classes.
///
/// Call this from your `build.rs`:
/// ```no_run
/// auto_jni::generate_bindings_file(
///     vec!["com.example.MyClass"],
///     Some("path/to/classes".into()),
///     &std::path::Path::new(&std::env::var("OUT_DIR").unwrap()).join("bindings.rs"),
///     Some(vec!["-Djava.class.path=path/to/classes".into()]),
/// ).unwrap();
/// ```
pub fn generate_bindings_file(
    classes: Vec<&str>,
    class_path: Option<String>,
    output_path: &Path,
    jvm_options: Option<Vec<String>>,
) -> std::io::Result<()> {
    let mut file = File::create(output_path)?;

    write_header(&mut file, jvm_options)?;

    for class in classes {
        let bindings = parse_javap_output(class, class_path.clone());
        write_class(&mut file, class, bindings)?;
    }

    Ok(())
}

// ---------------------------------------------------------------------------
// Header (JVM bootstrap + imports)
// ---------------------------------------------------------------------------

fn write_header(file: &mut File, jvm_options: Option<Vec<String>>) -> std::io::Result<()> {
    writeln!(file, "use auto_jni::jni::objects::{{JObject, GlobalRef}};")?;
    writeln!(file, "use auto_jni::jni::objects::{{JValue, JObjectArray}};")?;
    writeln!(file, "use auto_jni::jni::signature::{{Primitive, ReturnType}};")?;
    writeln!(file, "use auto_jni::jni::{{InitArgsBuilder, JNIEnv, JNIVersion, JavaVM}};")?;
    writeln!(file, "use auto_jni::lazy_static::lazy_static;")?;
    writeln!(file, "use auto_jni::errors::JNIError;")?;
    writeln!(file, "use auto_jni::{{call, call_static, create}};")?;
    writeln!(file)?;
    writeln!(file, "lazy_static! {{ static ref JAVA: JavaVM = create_jvm(); }}")?;
    writeln!(file)?;
    writeln!(file, "fn create_jvm() -> JavaVM {{")?;
    writeln!(file, "    let jvm_args = InitArgsBuilder::new()")?;
    writeln!(file, "        .version(JNIVersion::V8)")?;
    if let Some(opts) = jvm_options {
        for opt in opts {
            writeln!(file, "        .option(\"{}\")", opt.replace('\\', "\\\\"))?;
        }
    }
    writeln!(file, "        .build().unwrap();")?;
    writeln!(file, "    JavaVM::new(jvm_args).unwrap()")?;
    writeln!(file, "}}")?;
    writeln!(file)?;
    writeln!(file, "pub fn java() -> JNIEnv<'static> {{")?;
    writeln!(file, "    JAVA.attach_current_thread_permanently().unwrap()")?;
    writeln!(file, "}}")?;
    writeln!(file)
}

// ---------------------------------------------------------------------------
// Per-class struct + impl
// ---------------------------------------------------------------------------

fn write_class(
    file: &mut File,
    class: &str,
    bindings: Vec<crate::MethodBinding>,
) -> std::io::Result<()> {
    let struct_name = class.replace('.', "_");

    writeln!(file, "pub struct {} {{", struct_name)?;
    writeln!(file, "    inner: GlobalRef,")?;
    writeln!(file, "}}")?;
    writeln!(file)?;
    writeln!(file, "impl<'a> {} {{", struct_name)?;

    let mut seen_methods: HashMap<String, u32> = HashMap::new();
    let mut seen_enum_helpers: Vec<String> = Vec::new();

    for mut binding in bindings {
        // Generate a valueOf helper for each unique inner-class/enum arg type.
        for arg in &binding.args {
            if arg.contains('$') {
                // arg looks like "Lcom/example/Foo$Bar" — strip leading 'L'
                let enum_path = &arg[1..];
                if !seen_enum_helpers.contains(&enum_path.to_string()) {
                    seen_enum_helpers.push(enum_path.to_string());
                    write_enum_helper(file, enum_path)?;
                }
            }
        }

        // Strip lambda synthetic names (e.g. "lambda$foo$1" → "foo")
        if binding.name.contains('$') {
            let mut parts = binding.name.splitn(3, '$');
            parts.next(); // "lambda"
            binding.name = parts.next().unwrap_or("unknown").to_string();
        }

        let base_name = if binding.is_constructor {
            "new".to_string()
        } else {
            binding.name.clone()
        };

        // Disambiguate overloads by appending a counter suffix.
        let count = seen_methods.entry(base_name.clone()).or_insert(0);
        let method_name = if *count == 0 {
            base_name.clone()
        } else {
            format!("{}_{}", base_name, count)
        };
        *count += 1;

        write_method(file, &binding, &method_name)?;
    }

    // Accessor for the wrapped GlobalRef.
    writeln!(file, "    pub fn inner(&self) -> &GlobalRef {{")?;
    writeln!(file, "        &self.inner")?;
    writeln!(file, "    }}")?;
    writeln!(file, "}}")?;
    writeln!(file)
}

// ---------------------------------------------------------------------------
// Enum valueOf helper
// ---------------------------------------------------------------------------

fn write_enum_helper(file: &mut File, enum_path: &str) -> std::io::Result<()> {
    let fn_name = enum_path.replace('/', "_").replace('$', "_");
    writeln!(file, "    pub fn {}_from_str(s: &str) -> Result<JObject<'static>, JNIError> {{", fn_name)?;
    writeln!(file, "        Ok(call_static!(")?;
    writeln!(file, "            \"{}\",", enum_path)?;
    writeln!(file, "            \"valueOf\",")?;
    writeln!(file, "            \"(Ljava/lang/String;)L{};\",", enum_path)?;
    writeln!(file, "            &[JValue::Object(&java().new_string(s)?.into()).as_jni()],")?;
    writeln!(file, "            ReturnType::Object")?;
    writeln!(file, "        )?.l()?)")?;
    writeln!(file, "    }}")
}

// ---------------------------------------------------------------------------
// Method / constructor
// ---------------------------------------------------------------------------

fn write_method(
    file: &mut File,
    binding: &crate::MethodBinding,
    method_name: &str,
) -> std::io::Result<()> {
    let args: Vec<(String, String)> = binding.args.iter().enumerate()
        .map(|(i, t)| (format!("arg_{}", i), t.clone()))
        .collect();

    if binding.is_constructor {
        write_constructor(file, binding, method_name, &args)
    } else if binding.is_static {
        write_static_method(file, binding, method_name, &args)
    } else {
        write_instance_method(file, binding, method_name, &args)
    }
}

fn write_constructor(
    file: &mut File,
    binding: &crate::MethodBinding,
    method_name: &str,
    args: &[(String, String)],
) -> std::io::Result<()> {
    write!(file, "    pub fn {}(", method_name)?;
    write_arg_params(file, args)?;
    writeln!(file, ") -> Result<Self, JNIError> {{")?;
    writeln!(file, "        Ok(Self {{")?;
    write!(file, "            inner: create!(\"{}\", \"{}\", &[", binding.path, binding.signature)?;
    write_arg_values(file, args)?;
    writeln!(file, "])?")?;
    writeln!(file, "        }})")?;
    writeln!(file, "    }}")
}

fn write_static_method(
    file: &mut File,
    binding: &crate::MethodBinding,
    method_name: &str,
    args: &[(String, String)],
) -> std::io::Result<()> {
    let ret = get_return_type(&binding.return_type);
    let rust_ret = return_type_to_rust_str(ret.clone());

    write!(file, "    pub fn {}(", method_name)?;
    write_arg_params(file, args)?;
    writeln!(file, ") -> Result<{}, JNIError> {{", rust_ret)?;

    if ret == ReturnType::Primitive(Primitive::Void) {
        writeln!(file, "        call_static!(")?;
    } else {
        writeln!(file, "        let result = call_static!(")?;
    }
    writeln!(file, "            \"{}\",", binding.path)?;
    writeln!(file, "            \"{}\",", binding.name)?;
    writeln!(file, "            \"{}\",", binding.signature)?;
    write!(file, "            &[")?;
    write_arg_values(file, args)?;
    writeln!(file, "],")?;
    writeln!(file, "            {}", return_type_to_string(ret.clone()))?;
    writeln!(file, "        )?;")?;
    writeln!(file, "        Ok({})", unwrap_result(ret))?;
    writeln!(file, "    }}")
}

fn write_instance_method(
    file: &mut File,
    binding: &crate::MethodBinding,
    method_name: &str,
    args: &[(String, String)],
) -> std::io::Result<()> {
    let ret = get_return_type(&binding.return_type);
    let rust_ret = return_type_to_rust_str(ret.clone());

    write!(file, "    pub fn {}(&'a self", method_name)?;
    for (name, ty) in args {
        write!(file, ", {}: {}", name, java_type_to_rust(ty))?;
    }
    writeln!(file, ") -> Result<{}, JNIError> {{", rust_ret)?;

    if ret == ReturnType::Primitive(Primitive::Void) {
        writeln!(file, "        call!(")?;
    } else {
        writeln!(file, "        let result = call!(")?;
    }
    writeln!(file, "            self.inner.as_obj(),")?;
    writeln!(file, "            \"{}\",", binding.path)?;
    writeln!(file, "            \"{}\",", binding.name)?;
    writeln!(file, "            \"{}\",", binding.signature)?;
    write!(file, "            &[")?;
    write_arg_values(file, args)?;
    writeln!(file, "],")?;
    writeln!(file, "            {}", return_type_to_string(ret.clone()))?;
    writeln!(file, "        )?;")?;
    writeln!(file, "        Ok({})", unwrap_result(ret))?;
    writeln!(file, "    }}")
}

// ---------------------------------------------------------------------------
// Small helpers
// ---------------------------------------------------------------------------

fn write_arg_params(file: &mut File, args: &[(String, String)]) -> std::io::Result<()> {
    for (i, (name, ty)) in args.iter().enumerate() {
        if i > 0 { write!(file, ", ")?; }
        write!(file, "{}: {}", name, java_type_to_rust(ty))?;
    }
    Ok(())
}

fn write_arg_values(file: &mut File, args: &[(String, String)]) -> std::io::Result<()> {
    for (i, (name, ty)) in args.iter().enumerate() {
        if i > 0 { write!(file, ", ")?; }
        write!(file, "{}", jvalue_for(name, ty))?;
    }
    Ok(())
}

fn java_type_to_rust(ty: &str) -> &str {
    match ty {
        "I" => "i32",
        "J" => "i64",
        "D" => "f64",
        "F" => "f32",
        "Z" => "bool",
        "B" => "i8",
        "C" => "u16",
        "S" => "i16",
        t if t.starts_with('L') => "&JObject",
        t if t.starts_with('[') => "&JObjectArray",
        _ => "&JObject",
    }
}

fn jvalue_for(name: &str, ty: &str) -> String {
    match ty {
        "I" => format!("JValue::Int({}).as_jni()", name),
        "J" => format!("JValue::Long({}).as_jni()", name),
        "D" => format!("JValue::Double({}).as_jni()", name),
        "F" => format!("JValue::Float({}).as_jni()", name),
        "Z" => format!("JValue::Bool({} as u8).as_jni()", name),
        "B" => format!("JValue::Byte({}).as_jni()", name),
        "C" => format!("JValue::Char({}).as_jni()", name),
        "S" => format!("JValue::Short({}).as_jni()", name),
        _ => format!("JValue::Object({}).as_jni()", name),
    }
}

fn get_return_type(ty: &str) -> ReturnType {
    match ty {
        "I" => ReturnType::Primitive(Primitive::Int),
        "J" => ReturnType::Primitive(Primitive::Long),
        "D" => ReturnType::Primitive(Primitive::Double),
        "F" => ReturnType::Primitive(Primitive::Float),
        "Z" => ReturnType::Primitive(Primitive::Boolean),
        "B" => ReturnType::Primitive(Primitive::Byte),
        "C" => ReturnType::Primitive(Primitive::Char),
        "S" => ReturnType::Primitive(Primitive::Short),
        "V" => ReturnType::Primitive(Primitive::Void),
        _ => ReturnType::Object,
    }
}

fn return_type_to_rust_str(ret: ReturnType) -> &'static str {
    match ret {
        ReturnType::Primitive(Primitive::Int) => "i32",
        ReturnType::Primitive(Primitive::Long) => "i64",
        ReturnType::Primitive(Primitive::Double) => "f64",
        ReturnType::Primitive(Primitive::Float) => "f32",
        ReturnType::Primitive(Primitive::Boolean) => "bool",
        ReturnType::Primitive(Primitive::Byte) => "i8",
        ReturnType::Primitive(Primitive::Char) => "u16",
        ReturnType::Primitive(Primitive::Short) => "i16",
        ReturnType::Primitive(Primitive::Void) => "()",
        _ => "JObject<'static>",
    }
}

fn return_type_to_string(ret: ReturnType) -> &'static str {
    match ret {
        ReturnType::Primitive(Primitive::Int) => "ReturnType::Primitive(Primitive::Int)",
        ReturnType::Primitive(Primitive::Long) => "ReturnType::Primitive(Primitive::Long)",
        ReturnType::Primitive(Primitive::Double) => "ReturnType::Primitive(Primitive::Double)",
        ReturnType::Primitive(Primitive::Float) => "ReturnType::Primitive(Primitive::Float)",
        ReturnType::Primitive(Primitive::Boolean) => "ReturnType::Primitive(Primitive::Boolean)",
        ReturnType::Primitive(Primitive::Byte) => "ReturnType::Primitive(Primitive::Byte)",
        ReturnType::Primitive(Primitive::Char) => "ReturnType::Primitive(Primitive::Char)",
        ReturnType::Primitive(Primitive::Short) => "ReturnType::Primitive(Primitive::Short)",
        ReturnType::Primitive(Primitive::Void) => "ReturnType::Primitive(Primitive::Void)",
        _ => "ReturnType::Object",
    }
}

fn unwrap_result(ret: ReturnType) -> &'static str {
    match ret {
        ReturnType::Primitive(Primitive::Int) => "result.i().unwrap()",
        ReturnType::Primitive(Primitive::Long) => "result.j().unwrap()",
        ReturnType::Primitive(Primitive::Double) => "result.d().unwrap()",
        ReturnType::Primitive(Primitive::Float) => "result.f().unwrap()",
        ReturnType::Primitive(Primitive::Boolean) => "result.z().unwrap()",
        ReturnType::Primitive(Primitive::Byte) => "result.b().unwrap()",
        ReturnType::Primitive(Primitive::Char) => "result.c().unwrap()",
        ReturnType::Primitive(Primitive::Short) => "result.s().unwrap()",
        ReturnType::Primitive(Primitive::Void) => "()",
        _ => "result.l().unwrap()",
    }
}
