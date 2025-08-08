# xecut_bot

A Telegram bot for managing hackerspace visits and other things.

## Running

Create a config file `xecut_bot.yaml`. For example:

```yaml
telegram_bot:
  bot_token: ...
  public_chat_id: ...
  private_chat_id: ...
  public_channel_id: ...
db:
  sqlite_path: "xecut_bot.sqlite"
```

Create the database and nessesary tables:

```sh
cat migrations/* | sqlite3 xecut_bot.sqlite
```

Make sure you have Rust and Cargo installed (for example with rustup).

Then run:

```sh
cargo run --release
```

or build with `cargo build --release` and use the executable directly:

```sh
target/release/xecut_bot
```

## License
MIT
