# search6

get levels in the minecraft discord here: [search6.xyz](https://search6.xyz)

If you want to run this software yourself, it's avaliable as a docker container.
```ghcr.io/randomairborne/search6```.

## Development

Developing search6 - at least the html - is unfortunately nontrivial. You will need:

- [Rust](https://rustup.rs)
- [Redis](https://redis.io)

create a .env file with the root where your application will be served (trailing slashes are ignored),
and the URL to your redis instance, like so:

```dotenv
REDIS_URL=redis://localhost:6379/
ROOT_URL=http://localhost:8080/
```

then, you can run `cargo r` each time you change the HTML, and then reload your page.
