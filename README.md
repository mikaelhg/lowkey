# Lowkey, a low resource key/value HTTP database

Use a cheap low cpu/memory/disk VM to serve a simple key/value database over HTTP.

### Create a new user

```text
POST /users HTTP/1.1

username=foobar&password=barfoo
```

### Create a new database

```text
POST /:username

dbname=xyzzy
```

### Push a new anonymous document into a database

```text
PUT /:username/:dbname/
Content-type: text/json
Content-type: text/plain
Content-type: application/octet-stream

{"any": "valid", "json": "document", "or": ["you", "know"]}
```

### Push a new named document into a database

```text
PUT /:username/:dbname/:document
Content-type: text/json
Content-type: text/plain
Content-type: application/octet-stream

{"any": "valid", "json": "document", "or": ["you", "know"]}
Whatever, I do what I want!
```

### Get a value for a particular key inside a document

```text
GET /:username/:dbname/:document/:key
Accept: text/json

Response: ["you", "know"]
```