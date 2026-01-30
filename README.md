Halfway between `tail -f` and `lnav`

# Under construction...

## static link
```
pacman -S musl
rustup target add x86_64-unknown-linux-musl
cargo build --release --target x86_64-unknown-linux-musl   
```

## test
```
cargo test --test test
```

## debug logging
```
cargo build --features debug_log
```
