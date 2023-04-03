 fn main() {
fn main() {
	let foo;
	{
		let bar = 5;
		foo = &bar;
	}
	println!("{}", foo);
}

}