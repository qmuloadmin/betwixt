# Example

This is an example of the functionality of betwixt. 

<?btxt mode='overwrite' ?>

## Some source code
<?btxt filename='test.py'  ?>

You can insert source code, in any language, in your documentation.

```python
print("hello, world")
```

### Other languages

And, you can use any combination of languages you wish. 
<?btxt filename='test.sh' ?>

```bash
echo "hello world";
```

## How it works

Betwixt works similarly to other literate programming tools, like Emacs' `org mode` (and in fact, betwixt will, at a later date, work with org mode style markdown). 

These tools take a source file, usually a human-readable file with its own flavor of markdown, and _tangle_ the source blocks into one or more files, based on some metadata configuration.

<?btxt lang="btxt" filename="test.md" ?>
```md
<?btxt filename="test.lang" mode="overwrite" ?>
```python
print("hello world!")
\```
```

The example above shows how you attach betwixt properties to code blocks. Notice the `filename` property. This tells betwixt what file to write the contents of this block to.
