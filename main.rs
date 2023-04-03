 fn main() {
struct Foo {
	Foo: String,
	Bar: Vec<String>
}

fn main() {
	let foo = Foo{
		foo: "foo",
		bar: Vec::new()
	};
	do_the_thing(foo);
	// uncommenting this line will break compilation
	// println!("{}", foo.foo)
}

fn do_the_thing(foo Foo) {
	// This function now _owns_ foo
	// main() can no longer interact with foo at all
}

}