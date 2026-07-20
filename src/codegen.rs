use std::collections::HashMap;
use std::fs::File;
use std::io::Write;
use std::path::Path;

use jni::signature::{Primitive, ReturnType};

use crate::parse_javap_output;

/// Builds a Rust source file with JNI bindings for a set of Java classes.
///
/// Call this from your `build.rs`:
/// ```no_run
/// auto_jni::Builder::new()
///     .class("com.example.MyClass")
///     .class_path("path/to/classes")
///     .jvm_option("-Djava.class.path=path/to/classes")
///     .generate(&std::path::Path::new(&std::env::var("OUT_DIR").unwrap()).join("bindings.rs"))
///     .unwrap();
/// ```
#[derive(Default)]
pub struct Builder {
    classes: Vec<String>,
    class_path: Option<String>,
    jvm_options: Option<Vec<String>>,
    include_non_public: bool,
}

impl Builder {
    pub fn new() -> Self {
        Self::default()
    }

    /// Add a fully-qualified Java class (e.g. `"com.example.MyClass"`) to bind.
    pub fn class(mut self, class: impl Into<String>) -> Self {
        self.classes.push(class.into());
        self
    }

    /// Classpath to resolve `class()` entries against (passed to `javap -classpath`).
    pub fn class_path(mut self, class_path: impl Into<String>) -> Self {
        self.class_path = Some(class_path.into());
        self
    }

    /// Add a JVM option (e.g. `"-Djava.class.path=..."`) used when the generated code
    /// starts its embedded JVM.
    pub fn jvm_option(mut self, option: impl Into<String>) -> Self {
        self.jvm_options.get_or_insert_with(Vec::new).push(option.into());
        self
    }

    /// Include non-public members (private/protected/package-private fields and methods)
    pub fn include_non_public(mut self, include: bool) -> Self {
        self.include_non_public = include;
        self
    }

    /// Write the generated bindings to `output_path`.
    pub fn generate(self, output_path: &Path) -> std::io::Result<()> {
        let mut file = File::create(output_path)?;

        write_header(&mut file, self.jvm_options)?;

        for class in &self.classes {
            let (bindings, fields) = parse_javap_output(class, self.class_path.clone());
            write_class(&mut file, class, bindings, fields, self.include_non_public)?;
        }

        Ok(())
    }
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
    writeln!(file, "use auto_jni::{{call, call_static, create, get_field, set_field, get_static_field, set_static_field}};")?;
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
    fields: Vec<crate::FieldBinding>,
    include_non_public: bool,
) -> std::io::Result<()> {
    let struct_name = class.replace('.', "_");

    writeln!(file, "pub struct {} {{", struct_name)?;
    writeln!(file, "    inner: GlobalRef,")?;
    writeln!(file, "}}")?;
    writeln!(file)?;
    writeln!(file, "impl<'a> {} {{", struct_name)?;

    let mut seen_methods: HashMap<String, u32> = HashMap::new();
    let mut seen_enum_helpers: Vec<String> = Vec::new();

    let bindings = bindings.into_iter().filter(|b| include_non_public || b.is_public);
    let fields: Vec<_> = fields.into_iter().filter(|f| include_non_public || f.is_public).collect();

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
            to_snake_case(&binding.name)
        };

        let method_name = disambiguate(&mut seen_methods, &base_name);

        write_method(file, &binding, &method_name)?;
    }

    for field in &fields {
        let field_name = to_snake_case(&field.name);
        let getter_name = disambiguate(&mut seen_methods, &format!("get_{}", field_name));
        let setter_name = disambiguate(&mut seen_methods, &format!("set_{}", field_name));
        write_field(file, field, &getter_name, &setter_name)?;
    }

    // Accessor for the wrapped GlobalRef.
    writeln!(file, "    pub fn inner(&self) -> &GlobalRef {{")?;
    writeln!(file, "        &self.inner")?;
    writeln!(file, "    }}")?;
    writeln!(file, "}}")?;
    writeln!(file)
}

/// Convert a Java identifier to snake case.
fn to_snake_case(s: &str) -> String {
    let mut out = String::with_capacity(s.len() + 4);
    for (i, c) in s.chars().enumerate() {
        if c.is_uppercase() {
            if i > 0 {
                out.push('_');
            }
            out.extend(c.to_lowercase());
        } else {
            out.push(c);
        }
    }
    out
}

/// Disambiguate overloads/name clashes by appending a counter suffix.
fn disambiguate(seen: &mut HashMap<String, u32>, base_name: &str) -> String {
    let count = seen.entry(base_name.to_string()).or_insert(0);
    let name = if *count == 0 {
        base_name.to_string()
    } else {
        format!("{}_{}", base_name, count)
    };
    *count += 1;
    name
}

// ---------------------------------------------------------------------------
// Field getter/setter
// ---------------------------------------------------------------------------

fn write_field(
    file: &mut File,
    field: &crate::FieldBinding,
    getter_name: &str,
    setter_name: &str,
) -> std::io::Result<()> {
    let ret = get_return_type(&field.ty);
    let rust_ret = return_type_to_rust_str(ret.clone());
    let rust_arg_ty = java_type_to_rust(&field.ty);

    if field.is_static {
        writeln!(file, "    pub fn {}() -> Result<{}, JNIError> {{", getter_name, rust_ret)?;
        writeln!(file, "        let result = get_static_field!(\"{}\", \"{}\", \"{}\")?;", field.path, field.name, field.ty)?;
        writeln!(file, "        Ok({})", unwrap_result(ret.clone()))?;
        writeln!(file, "    }}")?;

        writeln!(file, "    pub fn {}(value: {}) -> Result<(), JNIError> {{", setter_name, rust_arg_ty)?;
        writeln!(file, "        Ok(set_static_field!(\"{}\", \"{}\", \"{}\", {})?)", field.path, field.name, field.ty, jvalue_expr("value", &field.ty))?;
        writeln!(file, "    }}")
    } else {
        writeln!(file, "    pub fn {}(&'a self) -> Result<{}, JNIError> {{", getter_name, rust_ret)?;
        writeln!(file, "        let result = get_field!(self.inner.as_obj(), \"{}\", \"{}\", \"{}\")?;", field.path, field.name, field.ty)?;
        writeln!(file, "        Ok({})", unwrap_result(ret.clone()))?;
        writeln!(file, "    }}")?;

        writeln!(file, "    pub fn {}(&'a self, value: {}) -> Result<(), JNIError> {{", setter_name, rust_arg_ty)?;
        writeln!(file, "        Ok(set_field!(self.inner.as_obj(), \"{}\", \"{}\", \"{}\", {})?)", field.path, field.name, field.ty, jvalue_expr("value", &field.ty))?;
        writeln!(file, "    }}")
    }
}

// ---------------------------------------------------------------------------
// Enum valueOf helper
// ---------------------------------------------------------------------------

fn write_enum_helper(file: &mut File, enum_path: &str) -> std::io::Result<()> {
    let fn_name = enum_path
        .split(['/', '$'])
        .map(to_snake_case)
        .collect::<Vec<_>>()
        .join("_");
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

/// A `JValue` construction expression for `name` of Java type `ty`.
fn jvalue_expr(name: &str, ty: &str) -> String {
    match ty {
        "I" => format!("JValue::Int({})", name),
        "J" => format!("JValue::Long({})", name),
        "D" => format!("JValue::Double({})", name),
        "F" => format!("JValue::Float({})", name),
        "Z" => format!("JValue::Bool({} as u8)", name),
        "B" => format!("JValue::Byte({})", name),
        "C" => format!("JValue::Char({})", name),
        "S" => format!("JValue::Short({})", name),
        _ => format!("JValue::Object({})", name),
    }
}

/// The raw `jni::sys::jvalue` form of [`jvalue_expr`], for method-call argument arrays.
fn jvalue_for(name: &str, ty: &str) -> String {
    format!("{}.as_jni()", jvalue_expr(name, ty))
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
