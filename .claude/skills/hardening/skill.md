# Hardening Skill

Security hardening skill - Automatically identify and fix unsafe patterns (unwrap/expect/assert) in code to improve reliability and security.

## Use Cases

Use this skill when you need to:
- Find panic-prone patterns in code (unwrap, expect, assert)
- Convert unsafe error handling to proper Result types
- Generate unit tests for new error paths
- Improve code safety and reliability

## Commands

```bash
/hardening                    # Scan current project and generate hardening report
/hardening <file_path>        # Apply hardening to specified file
/hardening --auto-fix         # Automatically fix all identified issues
```

## Workflow

### 1. Scanning Phase
- Use grep to find all `.unwrap()`, `.expect()`, `assert!()` patterns
- Analyze context to determine if fixes are needed (exclude legitimate test code usage)
- Generate prioritized report

### 2. Fixing Phase
- Modify function signatures to return `Result<T, Error>` types
- Replace `unwrap()` with `?` operator
- Replace `expect()` with `map_err()` and add descriptive error messages
- Replace `assert!()` with conditional checks and error returns
- Update all call sites to handle new Result return values

### 3. Testing Phase
- Generate unit tests for each new error path
- Test success scenarios and failure scenarios
- Test edge cases and boundary conditions

### 4. Verification Phase
- Run `cargo check` to ensure compilation succeeds
- Run `cargo clippy` to check code quality
- Run test suite to ensure functionality is preserved

## 示例

### 修复前
```rust
pub fn create_object(data: Data) -> Object {
    let field = data.into_field().unwrap();
    let obj = field.into_object().unwrap();
    obj
}
```

### 修复后
```rust
pub fn create_object(data: Data) -> Result<Object, Error> {
    let field = data.into_field()?;
    let obj = field.into_object()?;
    Ok(obj)
}

#[cfg(test)]
mod tests {
    #[test]
    fn test_create_object_success() {
        let data = Data::new();
        let result = create_object(data);
        assert!(result.is_ok());
    }

    #[test]
    fn test_create_object_invalid_data() {
        let data = Data::invalid();
        let result = create_object(data);
        assert!(result.is_err());
    }
}
```

## Configuration

Configure in `.claude/skills/hardening/config.json`:

```json
{
  "exclude_patterns": [
    "tests/**",
    "benches/**"
  ],
  "severity_threshold": "medium",
  "auto_generate_tests": true
}
```

## Best Practices

1. **Priority Ordering**:
   - High priority: unwrap/expect in production code
   - Medium priority: assert in library code
   - Low priority: legitimate usage in test code

2. **Error Messages**:
   - Use descriptive error messages
   - Include context information (e.g., specific values)
   - Make debugging and troubleshooting easier

3. **Test Coverage**:
   - Add tests for each error path
   - Test boundary conditions
   - Ensure error messages are clear

4. **Incremental Fixes**:
   - Start with files that have the most impact
   - Run tests after each fix
   - Gradually expand to the entire project

## Important Notes

- unwrap/expect in test code may be legitimate (tests should panic on failure)
- Some unwrap cases are safe (e.g., verified invariants) and require human judgment
- All call sites must be updated after fixes
- Choose appropriate error types (avoid overusing generic errors)

## Related Resources

- Rust Error Handling: https://doc.rust-lang.org/book/ch09-00-error-handling.html
- Clippy Lints: https://rust-lang.github.io/rust-clippy/master/index.html#unwrap_used
- Sui Hardening Examples: Reference `[hardening]` commits in the project
