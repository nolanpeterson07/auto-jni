pub mod errors;
mod call;

#[cfg(feature = "build")]
mod codegen;

#[cfg(feature = "build")]
pub use codegen::generate_bindings_file;

// Runtime re-exports used by generated code.
pub use jni;
pub use once_cell;
pub use lazy_static;

#[cfg(feature = "build")]
use regex::Regex;

#[cfg(feature = "build")]
#[derive(Debug, PartialEq)]
struct MethodBinding {
    path: String,
    name: String,
    signature: String,
    args: Vec<String>,
    return_type: String,
    is_static: bool,
    is_constructor: bool,
}

#[cfg(feature = "build")]
pub(crate) fn parse_javap_output(class_name: &str, class_path: Option<String>) -> Vec<MethodBinding> {
    use std::process::Command;

    let mut command = Command::new("javap");
    command.args(["-s", "-p"]);

    if let Some(cp) = class_path {
        command.arg("-classpath").arg(cp);
    }

    command.arg(class_name);

    let output = command.output().expect("Failed to execute javap");
    let output_str = String::from_utf8_lossy(&output.stdout);

    let simple_class_name = class_name.split('.').last().unwrap_or(class_name);

    // Group 1 = static modifier
    // Group 2 = "ReturnType MethodName" for regular methods, or just "com.example.ClassName" for constructors.
    // We split group 2 on whitespace and take the last token, then strip any qualifier via '.'.
    let method_regex = Regex::new(
        r"(?m)^\s*(?:public|private|protected)?\s*(static\s+native|native\s+static|static|native)?\s*([\w$<>\[\].]+(?:\s+[\w$<>]+)?)\s*\(([^)]*)\)\s*(?:throws\s+[\w.,\s]+)?\s*;"
    ).unwrap();
    let descriptor_regex = Regex::new(r"^\s*descriptor:\s*(.+)$").unwrap();

    let mut bindings = Vec::new();
    let mut lines = output_str.lines().peekable();

    while let Some(line) = lines.next() {
        if let Some(captures) = method_regex.captures(line) {
            let is_static = captures.get(1).map_or("", |m| m.as_str()).contains("static");
            let combined = captures.get(2).map_or("", |m| m.as_str());
            let last_token = combined.split_whitespace().last().unwrap_or(combined);
            let name = last_token.split('.').last().unwrap_or(last_token).to_string();
            let is_constructor = name == simple_class_name;

            while let Some(next_line) = lines.peek() {
                if let Some(desc_captures) = descriptor_regex.captures(next_line) {
                    let signature = desc_captures.get(1).map_or("", |m| m.as_str()).to_string();
                    let args = parse_descriptor_args(&signature);
                    let return_type = parse_descriptor_return(&signature);

                    bindings.push(MethodBinding {
                        path: class_name.replace('.', "/"),
                        name: name.clone(),
                        signature,
                        args,
                        return_type,
                        is_static,
                        is_constructor,
                    });
                    break;
                }
                lines.next();
            }
        }
    }

    bindings
}

#[cfg(feature = "build")]
fn parse_descriptor_args(descriptor: &str) -> Vec<String> {
    let args_section = descriptor
        .trim_start_matches('(')
        .split(')')
        .next()
        .unwrap_or("");

    let mut args = Vec::new();
    let mut chars = args_section.chars().peekable();

    while let Some(c) = chars.next() {
        match c {
            'L' => {
                let mut class_name = String::new();
                while let Some(nc) = chars.next() {
                    if nc == ';' { break; }
                    class_name.push(nc);
                }
                args.push(format!("L{}", class_name));
            }
            'I' | 'J' | 'D' | 'F' | 'B' | 'C' | 'S' | 'Z' => args.push(c.to_string()),
            '[' => {
                let mut array_type = String::from("[");
                if let Some(next_char) = chars.next() {
                    array_type.push(next_char);
                    if next_char == 'L' {
                        while let Some(nc) = chars.next() {
                            array_type.push(nc);
                            if nc == ';' { break; }
                        }
                    }
                }
                args.push(array_type);
            }
            _ => continue,
        }
    }

    args
}

#[cfg(feature = "build")]
fn parse_descriptor_return(descriptor: &str) -> String {
    descriptor.split(')').nth(1).unwrap_or("").to_string()
}

#[cfg(all(test, feature = "build"))]
mod tests {
    use super::*;

    #[test]
    fn test_parse_descriptor() {
        assert_eq!(parse_descriptor_args("(II)I"), vec!["I", "I"]);
        assert_eq!(
            parse_descriptor_args("(ILjava/lang/String;[I)V"),
            vec!["I", "Ljava/lang/String", "[I"]
        );
        assert_eq!(parse_descriptor_return("(II)I"), "I");
        assert_eq!(
            parse_descriptor_args("(Lcom/example/EnumTest$CountEnum;)I"),
            vec!["Lcom/example/EnumTest$CountEnum"]
        );
        assert_eq!(parse_descriptor_return("(Lcom/example/EnumTest$CountEnum;)I"), "I");
    }

    #[test]
    fn test_parse_car() {
        // Fixture .class isn't checked in; compile it before javap needs it.
        let status = std::process::Command::new("javac")
            .args(["-d", "examples/java/src", "examples/java/src/com/example/Car.java"])
            .status()
            .expect("Failed to run javac");
        assert!(status.success(), "javac failed to compile fixture");

        let bindings = parse_javap_output(
            "com.example.Car",
            Some("examples/java/src".to_string()),
        );
        assert!(!bindings.is_empty(), "No bindings parsed");

        let ctor = bindings.iter().find(|b| b.is_constructor).expect("No constructor");
        assert_eq!(ctor.path, "com/example/Car");
        assert_eq!(ctor.signature, "(Ljava/lang/String;Ljava/lang/String;ILcom/example/Car$CarType;)V");

        assert!(bindings.iter().any(|b| b.name == "getMake"));
        assert!(bindings.iter().any(|b| b.name == "displayInfo"));
    }
}
