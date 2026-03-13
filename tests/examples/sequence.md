# Sequence Test

This test ensures that if we append an anchor in one block, we can fill it in a subsequent block in the same run.

<?btxt filename='seq.rs' mode='overwrite' ?>
```rust
fn main() {
    println!("Start");
    // <btxt anchor="dynamic"
    // btxt>
    println!("End");
}
```

<?btxt filename='seq.rs' mode='append' anchor='dynamic' ?>
```rust
    println!("Middle");
```
