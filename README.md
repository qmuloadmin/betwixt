<?btxt+btxt ignore=true ?>
# Betwixt

Simple, markdown-based, polyglot literate programming and documentation tests. 

> Read code between the lines

## Summary

Betwixt is heavily inspired by the literate programming features of Emacs Org Mode. The ability to write documentation intended for humans, with easily-consumable formatting, emphasis and organization, and embed code examples or even API call examples in that documentation, and have those examples be executable tests means documentation never gets out of date -- if you change the code, the documentation _is the test_ and so you must update the documentation to pass. 

Betwixt extracts code segments from README files (currently, only github flavor is supported, but broader support is planned) and _tangles_ them into various source files as configured, allowing them to be built and executed as a part of the CI/CD pipeline, causing failure if the documentation is out of date, or simply allowing entire programs to be written in a format primarily suitable for human consumption, instead of the opposite.

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

This is going to configure betwixt to copy all code segments of all languages into a file called "test.py". You may use either single quotes `'` or double quotes `"` for property values. You may also use three pipe operators (`|||`) if you need to embed code in a property.

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
 
 While it is not treated as a normal property, you can also set a `code` property in a betwixt block. This is never inherited, and it is effectively treated as a code block for tangle operations. The difference is that it isn't visible in the rendered markdown -- this is useful for internal plumbing or boilerplate you don't want the end users seeing.
 
#### Scope

Properties are defined with a scope of markdown headings. Parent headings' properties are inherited by children, but don't affect siblings or parents.

#### Example

<?btxt+go filename="main.go" tag='examples' code=|||package main

import "fmt"

func main() {
||| mode='overwrite' ?>

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

## State and Plans

Betwixt is still very, painfully premature. It does technically work, but it is going to be very rough around the edges, with unhelpful crash error messages in the case of a misconfigured markdown source. It'll also likely have a few fundamental bugs, and maybe even (*gasp*) some bad design decisions. Use at your own risk at the moment.

Ultimately, I want betwixt to have the following features before I will consider it complete:

 - [ ] Strict mode to prevent you from doing some things you probably don't intend to (e.g. source blocks that are never tangled)
 - [ ] Prefix and Postfix code properties
 - [ ] Simple test runner to create temp directories, execute commands, output success or failure, and cleanup
 - [ ] Insert mode to insert code blocks into a specific point in an existing file
 - [ ] More Markdown flavors and Org Mode syntax support
 - [ ] Support tangling from multiple markdown documents in a heirarchy
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
