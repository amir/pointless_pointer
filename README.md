# pointless-pointer

## Point out pointless overrides in Yaml documents

```bash
$ cat base.yaml 
database:
  username: "foo"

database:
  username: "foo1"

$ cat first.yaml 
database:
  password: "bar"

❯ cat second.yaml 
database:
  username: "foo1"
  password: "bar2"

$ cat third.yaml 
---
database:
  password: "bar2"


$ pointless_pointer base.yaml -f first.yaml -f second.yaml -f third.yaml
⚠ Warnings - Duplicate keys with different values in the same document:
  Suggestion: Consider keeping only one

  File: /tmp/base.yaml
  Path: database.username
  First value: foo (line 2)
  Second value: foo1 (line 5)

Warning summary: 1 duplicate key warning(s)

⚠ Found pointless overrides:

  File: /tmp/second.yaml:2
  Path: database.username
  Value: foo1
  Same as: foo1 (from /tmp/base.yaml:5)

  File: /tmp/third.yaml:3
  Path: database.password
  Value: bar2
  Same as: bar2 (from /tmp/second.yaml:3)

Summary: 2 pointless override(s) found
```
