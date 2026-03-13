use std::process::Command;
use std::fs;
use std::path::PathBuf;

#[test]
fn test_basic_tangle() {
    let root = PathBuf::from(env!("CARGO_MANIFEST_DIR"));
    let exe = root.join("target/debug/betwixt");
    let input = root.join("tests/examples/basic.md");
    let out_dir = root.join("tests/tmp/basic");

    if out_dir.exists() {
        fs::remove_dir_all(&out_dir).unwrap();
    }
    fs::create_dir_all(&out_dir).unwrap();

    let output = Command::new(exe)
        .arg(input)
        .arg("-o")
        .arg(&out_dir)
        .output()
        .expect("failed to execute process");

    if !output.status.success() {
        eprintln!("stdout: {}", String::from_utf8_lossy(&output.stdout));
        eprintln!("stderr: {}", String::from_utf8_lossy(&output.stderr));
    }
    assert!(output.status.success());
    
    let script_sh = out_dir.join("script.sh");
    assert!(script_sh.exists());
    let content = fs::read_to_string(script_sh).unwrap();
    assert_eq!(content, "echo \"Hello from Shell\"\n");

    let script_py = out_dir.join("script.py");
    assert!(script_py.exists());
    let content = fs::read_to_string(script_py).unwrap();
    assert_eq!(content, "print(\"Hello from Python\")\n");

    fs::remove_dir_all(&out_dir).unwrap();
}

#[test]
fn test_tag_filtering() {
    let root = PathBuf::from(env!("CARGO_MANIFEST_DIR"));
    let exe = root.join("target/debug/betwixt");
    let input = root.join("tests/examples/tags.md");
    let out_dir = root.join("tests/tmp/tags");

    if out_dir.exists() {
        fs::remove_dir_all(&out_dir).unwrap();
    }
    fs::create_dir_all(&out_dir).unwrap();

    // Tangle only with tag 'foo'
    let output = Command::new(&exe)
        .arg(&input)
        .arg("-o")
        .arg(&out_dir)
        .arg("-t")
        .arg("foo")
        .output()
        .expect("failed to execute process");

    if !output.status.success() {
        eprintln!("stdout: {}", String::from_utf8_lossy(&output.stdout));
        eprintln!("stderr: {}", String::from_utf8_lossy(&output.stderr));
    }
    assert!(output.status.success());
    
    let foo_txt = out_dir.join("foo.txt");
    assert!(foo_txt.exists());
    let bar_txt = out_dir.join("bar.txt");
    assert!(!bar_txt.exists());

    // Tangle only with tag 'bar'
    let output = Command::new(&exe)
        .arg(&input)
        .arg("-o")
        .arg(&out_dir)
        .arg("-t")
        .arg("bar")
        .output()
        .expect("failed to execute process");

    if !output.status.success() {
        eprintln!("stdout: {}", String::from_utf8_lossy(&output.stdout));
        eprintln!("stderr: {}", String::from_utf8_lossy(&output.stderr));
    }
    assert!(output.status.success());
    assert!(bar_txt.exists());

    fs::remove_dir_all(&out_dir).unwrap();
}

#[test]
fn test_execution() {
    let root = PathBuf::from(env!("CARGO_MANIFEST_DIR"));
    let exe = root.join("target/debug/betwixt");
    let input = root.join("tests/examples/exec.md");
    let out_dir = root.join("tests/tmp/exec");

    if out_dir.exists() {
        fs::remove_dir_all(&out_dir).unwrap();
    }
    fs::create_dir_all(&out_dir).unwrap();

    let output = Command::new(&exe)
        .arg(&input)
        .arg("-o")
        .arg(&out_dir)
        .arg("-e")
        .arg("mypy,mysh")
        .output()
        .expect("failed to execute process");

    if !output.status.success() {
        eprintln!("stdout: {}", String::from_utf8_lossy(&output.stdout));
        eprintln!("stderr: {}", String::from_utf8_lossy(&output.stderr));
    }
    assert!(output.status.success());
    let stdout = String::from_utf8_lossy(&output.stdout);
    assert!(stdout.contains("Executed via Python3"));
    assert!(stdout.contains("Executed via Sh"));
    
    assert!(out_dir.join("hello.py").exists());
    assert!(out_dir.join("hello.sh").exists());

    fs::remove_dir_all(&out_dir).unwrap();
}

#[test]
fn test_nested_flavor() {
    let root = PathBuf::from(env!("CARGO_MANIFEST_DIR"));
    let exe = root.join("target/debug/betwixt");
    let input = root.join("tests/examples/nested.md");
    let out_dir = root.join("tests/tmp/nested");

    if out_dir.exists() {
        fs::remove_dir_all(&out_dir).unwrap();
    }
    fs::create_dir_all(&out_dir).unwrap();

    let output = Command::new(&exe)
        .arg(&input)
        .arg("-o")
        .arg(&out_dir)
        .arg("--flavor")
        .arg("nested")
        .output()
        .expect("failed to execute process");

    if !output.status.success() {
        eprintln!("stdout: {}", String::from_utf8_lossy(&output.stdout));
        eprintln!("stderr: {}", String::from_utf8_lossy(&output.stderr));
    }
    assert!(output.status.success());
    
    let nested_txt = out_dir.join("nested.txt");
    assert!(nested_txt.exists());
    let content = fs::read_to_string(nested_txt).unwrap();
    assert_eq!(content, "This is nested.\n");

    fs::remove_dir_all(&out_dir).unwrap();
}

#[test]
fn test_prefix_postfix() {
    let root = PathBuf::from(env!("CARGO_MANIFEST_DIR"));
    let exe = root.join("target/debug/betwixt");
    let input = root.join("tests/examples/wrapped.md");
    let out_dir = root.join("tests/tmp/wrapped");

    if out_dir.exists() {
        fs::remove_dir_all(&out_dir).unwrap();
    }
    fs::create_dir_all(&out_dir).unwrap();

    let output = Command::new(&exe)
        .arg(&input)
        .arg("-o")
        .arg(&out_dir)
        .output()
        .expect("failed to execute process");

    if !output.status.success() {
        eprintln!("stdout: {}", String::from_utf8_lossy(&output.stdout));
        eprintln!("stderr: {}", String::from_utf8_lossy(&output.stderr));
    }
    assert!(output.status.success());
    
    let wrapped_txt = out_dir.join("wrapped.txt");
    assert!(wrapped_txt.exists());
    let content = fs::read_to_string(wrapped_txt).unwrap();
    assert_eq!(content, "<start>Middle\n<end>");

    fs::remove_dir_all(&out_dir).unwrap();
}

#[test]
fn test_anchors() {
    let root = PathBuf::from(env!("CARGO_MANIFEST_DIR"));
    let exe = root.join("target/debug/betwixt");
    let input = root.join("tests/examples/anchors.md");
    let out_dir = root.join("tests/tmp/anchors");

    if out_dir.exists() {
        fs::remove_dir_all(&out_dir).unwrap();
    }
    fs::create_dir_all(&out_dir).unwrap();

    // Create a file with anchors first
    let initial_content = "
fn main() {
    // @btxt anchor=\"foo\"
    // btxt@
    // @btxt anchor=\"bar\"
    OLD BAR
    // btxt@
}
";
    let target_path = out_dir.join("anchored.rs");
    fs::write(&target_path, initial_content).unwrap();

    let output = Command::new(&exe)
        .arg(&input)
        .arg("-o")
        .arg(&out_dir)
        .output()
        .expect("failed to execute process");

    if !output.status.success() {
        eprintln!("stdout: {}", String::from_utf8_lossy(&output.stdout));
        eprintln!("stderr: {}", String::from_utf8_lossy(&output.stderr));
    }
    assert!(output.status.success());
    
    let content = fs::read_to_string(target_path).unwrap();
    assert!(content.contains("println!(\"Hello Anchor!\");"));
    assert!(content.contains("println!(\"Hello Bar!\");"));
    assert!(content.contains("println!(\"Hello Bar Again!\");"));
    assert!(!content.contains("OLD BAR"));

    fs::remove_dir_all(&out_dir).unwrap();
}

#[test]
fn test_sequence_with_anchors() {
    let root = PathBuf::from(env!("CARGO_MANIFEST_DIR"));
    let exe = root.join("target/debug/betwixt");
    let input = root.join("tests/examples/sequence.md");
    let out_dir = root.join("tests/tmp/sequence");

    if out_dir.exists() {
        fs::remove_dir_all(&out_dir).unwrap();
    }
    fs::create_dir_all(&out_dir).unwrap();

    let output = Command::new(&exe)
        .arg(&input)
        .arg("-o")
        .arg(&out_dir)
        .output()
        .expect("failed to execute process");

    if !output.status.success() {
        eprintln!("stdout: {}", String::from_utf8_lossy(&output.stdout));
        eprintln!("stderr: {}", String::from_utf8_lossy(&output.stderr));
    }
    assert!(output.status.success());
    
    let content = fs::read_to_string(out_dir.join("seq.rs")).unwrap();
    assert!(content.contains("println!(\"Start\");"));
    assert!(content.contains("println!(\"Middle\");"));
    assert!(content.contains("println!(\"End\");"));
    
    // Check order
    let start_pos = content.find("Start").unwrap();
    let middle_pos = content.find("Middle").unwrap();
    let end_pos = content.find("End").unwrap();
    assert!(start_pos < middle_pos);
    assert!(middle_pos < end_pos);

    fs::remove_dir_all(&out_dir).unwrap();
}
