fn main() {
	println!("cargo:rustc-link-search=ofs-convert/build");

	println!("cargo:rustc-link-lib=static=ofs-convert");
	println!("cargo:rustc-link-lib=static=uuid");
	println!("cargo:rustc-link-lib=static=stdc++");
}
