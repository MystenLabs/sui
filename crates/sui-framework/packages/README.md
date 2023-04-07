---

```
/Users/rijnard/sui/target/debug/sui move test --coverage
```

in `sui-framework` and `sui-system`


---

```
cd sui-framework && python3 ../cov.py sui-framework > ~/sui-move-package-test/sui-framework.json
cd sui-system && python3 ../cov.py sui-system > ~/sui-move-package-test/sui-system.json
```

^ merge these coverage reports

---

python3 dis.py
