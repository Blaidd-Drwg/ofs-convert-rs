use cmake;

fn main() {
	let dst = cmake::Config::new("ofs-convert")
		.build_target("ofs-convert")
		.build();

	println!("cargo:rustc-link-search={}/build", dst.display());

	println!("cargo:rustc-link-lib=static=ofs-convert");
	println!("cargo:rustc-link-lib=static=uuid");
	println!("cargo:rustc-link-lib=static=stdc++");
}
