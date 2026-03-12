# Execute with Python

This example tangles a python script and executes it using the `cmd` property.

```python mypy cmd=|||python3 hello.py||| filename='hello.py'
print("Executed via Python3")
```

# Execute with Shell

```sh mysh cmd=|||sh hello.sh||| filename='hello.sh'
echo "Executed via Sh"
```
