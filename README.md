<?btxt+btxt ignore=true ?>
<?btxt mode='overwrite' ?>
# Betwixt

Simple, markdown-based, polyglot literate programming and documentation tests. 

> Read code between the lines

## Summary

Betwixt is heavily inspired by the literate programming features of Emacs Org Mode. The ability to write documentation intended for humans, with easily-consumable formatting, emphasis and organization, and embed code examples or even API call examples in that documentation, and have those examples be executable tests means documentation never gets out of date -- if you change the code, the documentation _is the test_ and so you must update the documentation to pass. 

Betwixt extracts code segments from markdown files (currently, only github flavor is supported, but broader support is planned) and _tangles_ them into various source files as configured, allowing them to be built and executed as a part of the CI/CD pipeline, causing failure if the documentation is out of date, or simply allowing entire programs to be written in a format primarily suitable for human consumption, instead of the opposite.

## Installation

At the moment, the only way to install `betwixt` is from source. As it gets more mature, it may get put in `crates.io` or a couple different distrobution package repositories. 

Betwixt is built in rust. You will need to [install rust](https://www.rust-lang.org/learn/get-started) first. Then, clone this project, and `cargo build --release`. You can copy the built executable anywhere in your path.

## Usage 

To use betwixt, you will first need a markdown file. The one you're reading now is just fine. Then, you'll need to add betwixt configuration commands to your file. This file already has a few, so you can safely use it for demonstration purposes.

### Configuring Code Segments

Betwixt configuration is accomplished using markdown comments of a certain format. You can set properties, which will be applied to _all following_ code blocks, until overwritten by other, more specific or more recent blocks. Below is a _global_ block, which just means it applies to code blocks of all languages, by default.

```btxt
<?btxt filename='test.py' ?>
```

This is going to configure betwixt to copy all code segments of all languages into a file called "test.py". You may use either single quotes `'` or double quotes `"` for property values. You may also use three pipe operators (`|||`) if you need to embed code that contains quotes in a property.

Note that only properties with string values need or accept quotes. Properties that take boolean values (like `ignore`) take the literal `true` or `false` without quotes. Hopefully this is intuitive to most users.

You can also configure properties that only apply to code blocks of a certain language.

```btxt
<?btxt+python filename='test.py' ?>
```

Which usually makes more sense, unless all your code blocks in a given file will be the same language. 

#### Properties

Currently, you can set the following properties in a betwixt block:

 - `filename` which indicates the file to which the code blocks should be written to. This should be a relative path.
 - `mode` indicates the write mode for writing to the files. By default it is `append`. Currently also supported is `overwrite`.
 - `tag` sets a tag, just a string, on the code block(s). This allows filtering on the command line to only tangle code with a certain tag. Additional functionality around tags is likely coming soon.
 - `ignore` indicates that the code block should not be tangled, and should be left alone
 - `prefix` sets a code block to be written to file _before_ contents in visible code blocks are written. This is good for hiding boilerplate.
 - `postfix` sets a code block to be written to file _after_ contents in visible code blocks are written.
 
 While it is not treated as a normal property, you can also set a `code` property in a betwixt block. This is never inherited, and it is effectively treated as a code block for tangle operations. The difference is that it isn't visible in the rendered markdown -- this is useful for internal plumbing or boilerplate you don't want the end users seeing.
 
#### Scope

<?btxt+btxt ignore=false filename='scope.md' tag='scope' ?>

Properties are defined with a scope of markdown headings. Parent headings' properties are inherited by children, but don't affect siblings or parents. Global properties (properties with no language set) override unset values on properties with a language set. This should hopefully be intuitive. 

#### Scope Example

Consider the following markdown source. There are not many code blocks here, we are simply focusing on betwixt blocks for properties. Note that the code blocks in this example have the triple backticks "`" replaced with triple single quotes. This is to allow code blocks to reside in github code blocks without further changes. 

```btxt
The root of the document (no explicit headings set yet) is the parent of all headings
The below betwixt block sets a global (no set language) `mode` property
<?btxt mode='overwrite' ?>

And the below block sets a filename for blocks that are _python_
<?btxt+python filename="foo.py" ?>

# A Level Down
Because we are now in a child heading, properties are all inherited.
At this point, all code blocks have mode='overwrite' and python blocks will have filename='foo.py' set
<?btxt+python tag="a" ?>
	
All code blocks below the above block will have the 'tag' property set to `a`

# A Child level
Code blocks at this point would lost the `a` "tag" property, since that was set in a sibling.
However, we still have the properties from the root/parent, so we still have `mode` set to "overwrite"
	
<?btxt+python filename="bar.py" ?>
	
Any python code blocks from this point on in this heading level would now be set to write to "bar.py"
	
## A Nested Child
	
Since this is a child level, all code blocks in this section will receive the properties set in "A Child Level" and the root, 
so properties at this point look like this:

 - mode="overwrite" for _all_ blocks
 - filename="bar.py" for python blocks
 
<?btxt tag="b" ?>
Now _all_ code blocks (regardless of langauge) have a tag property of "b" in this section (and any children)

'''python
# this has the tag 'b' and will write to "bar.py"
print("Hello, Betwixt!")
'''

# Another Child Level
Okay, now we've dropped our "A Nested Child" and gone _up_ a level. Any properties set on the sibling and child are now gone. This means that properties look like this:

- mode="overwrite" for _all_ blocks
- filename="foo.py" for python blocks

'''python
print("Hello Foo File!")
'''

Note that you can never reach the root level of the document once left, so properties set in the root are truly global
```

#### Hidden Code Example

<?btxt+go filename="main.go" tag='examples' code=|||package main

import "fmt"

func main() {
||| ?>

For an example, look at the source of this markdown file compared to the rendered version. This markdown file is a simple but complete example of betwixt. The below code segment can be tangled into a source file that is executable, even though it isn't a complete, valid `golang` program by itself.

<?btxt+go mode='append' ?>
```go
fmt.Println("Hello, Betwixt!")
```

<?btxt+go code='}' ?>

### Tangling Markdown

To tangle you just need to provide the markdown filename, and a destination output directory. You can use this README as the source.

`betwixt README.md -o /tmp/`

If you run the above command in the root of this repository, you can then see a complete (albeit painfully simple) go program in `/tmp/main.go`. If you have go installed, you can execute it with `go run /tmp/main.go`

#### Command Line Options

You can use `--help` to get more information on the command line options. This will always be the best place to go for up-to-date usage information. In general, the most useful parameters are:

- `o` or `--outpath` to set the directory to write tangled files to. If a `filename` prop is set to `foo.txt` and `-o` is set to `/tmp/` then code will tangle to `/tmp/foo.txt`
- `t` to filter by a tag. Only code blocks with that tag set will be tangled
- `--flavor` will set an optional Markdown flavor. This changes parsing tokens. Right now only `github` is supported, and is the default value. In order to support nested markdown, there is also the `nested` flavor, which is primarily there to allow betwixt to eat its own dog food. 

## State and Plans

Betwixt is still very, painfully premature. It does technically work, but it is going to be very rough around the edges, with unhelpful crash error messages in the case of a misconfigured markdown source. It'll also likely have a few fundamental bugs, and maybe even (*gasp*) some bad design decisions. Use at your own risk at the moment.

Ultimately, I want betwixt to have the following features before I will consider it complete:

 - [x] Strict mode to prevent you from doing some things you probably don't intend to (e.g. source blocks that are never tangled)
 - [x] Prefix and Postfix code properties
 - [ ] Clear and helpful error messages with line numbers
 - [ ] Unicode-aware parsing instead of bytes with several unicode encoding support
 - [ ] Simple test runner to create temp directories, execute commands, output success or failure, and cleanup
 - [ ] Insert mode to insert code blocks into a specific point in an existing file
 - [ ] More Markdown flavors and Org Mode syntax support
 - [ ] Support tangling from multiple markdown documents in a heirarchy (e.g. an Obsidian vault)
 - [ ] The ability to execute code blocks by tag or id and put the results in the MD file (a la org-babel)
 - [ ] Extension of above, interpolation to allow execution of one block to be input or variable to another block (a la org-babel). This will likely be more simplistic than OB's version.
 
### Wait, Tangles?

`Tangle` is a fancy word for writing out all the different code segments in the documentation into the appropriate places in source files. The opposite is _untangled_, which is the documentation. This word comes from [literate programming](http://www.literateprogramming.com/) jargon.

### Why not just X Language's documentation tests?

A lot of modern languages, and even some older ones, support embedding tests inside doc strings or other comments in the source code. This is a great system, however I find myself using them infrequently. Each language is a little (or even a lot) different from the last, and getting all engineers to use embedded documentation in some languages is harder than others. If documentation tests in your language are working for you, then don't let me stop you. 

I also feel like they primarly fill a different need. They are necessary and great for technical documentation on how to use the API of a library. But they are perhaps not great for tutorials, guides that are _not_ about the program or service itself (but perhaps about its REST API), or anything where you can't expect the user to understand the language of the source itself, but still want to ensure up to date documentation.

### Why not just Emacs Org Mode?

The primary problem with Org Mode is that it only runs in emacs. And even with support for things like emacs batch mode, its not very portable. Its difficult to get a team of developers to all use it. Markdown, though, is ubiquitous. Almost every editor has support for it, most version control UIs (like this one) have support for rendering... But there are not many options for turning markdown into a literate programming tool.

## Contributing

Feel free to create an issue with any feedback. At the moment, things are so early stage I'm not super willing to just open the floodgates to direct contribution -- nor would I expect anyone would want to dive into this (albeit small) codebase right now. 
