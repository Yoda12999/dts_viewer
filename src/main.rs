use std::env;
use std::process::Command;
use std::path::{Path, PathBuf};

use inner_tree::*;
use cpp_parser::*;

mod cpp_parser;
mod inner_tree;

// Device Tree stucture
/*
struct DeviceTree<'a> {
	version: u32,
	boot_cpuid: u32,
	reserved_mem: Vec<(u64, u64)>,
	root: Node<'a, 'a>,
}

struct DeviceNode<'a, 'b> {
	name: String,
	props: Vec<(String, Property<'a>)>,
	children: Vec<DeviceNode<'b, 'b>>,
}

enum Property<'a> {
	Empty,
	Cells(Vec<u32>),
	String(String),
	ByteString(Vec<u8>),
	Combo(Vec<&'a Property<'a>>),
}
*/

// Change tracking
/*
struct Change<'a> {
	file: File,
	line: usize,
	// maybe point to node?
	name: String,
	value: Property<'a>,
}
*/

const CPP_OUTPUT_NAME: &'static str = "dts_viewer_tmp.dts";

fn main() {
	let file_name = match env::args().nth(1) {
		None => {
			println!("You forgot the dts file, you dummy");
			return;
		}
		Some(x) => x,
	};

	let arch = "arm";

	let dts_folder = PathBuf::from("arch").join(arch).join("boot/dts/");
	let file_path = dts_folder.join(file_name);

	let include_output = Command::new("arm-linux-gnueabi-gcc")
		.args(&["-H", "-E", "-nostdinc"])
		.args(&["-I", dts_folder.to_str().unwrap()])
		.args(&["-I", dts_folder.join("include/").to_str().unwrap()])
		.args(&["-undef", "-D__DTS__", "-x", "assembler-with-cpp"])
		.args(&["-o", CPP_OUTPUT_NAME])
		.arg(&file_path)
		.output()
		.expect("failed to execute process"); //TODO: properly handle errors

	let cpp_stderr = String::from_utf8_lossy(&include_output.stderr);
	println!("{}", cpp_stderr);

	let mut root_file = ParsedFile::new(&Path::new(&file_path), IncludeMethod::CPP(Vec::new()));

	parse_cpp_outputs(&cpp_stderr, Path::new(CPP_OUTPUT_NAME), &mut root_file);

	println!("{}", root_file);
}
