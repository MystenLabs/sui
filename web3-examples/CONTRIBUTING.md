# Contributing to Web3 Multi-Language Playground

Thank you for your interest in contributing! This document provides guidelines for contributing to this project.

## Getting Started

1. Fork the repository
2. Clone your fork: `git clone https://github.com/your-username/sui.git`
3. Create a feature branch: `git checkout -b feature/amazing-feature`
4. Make your changes
5. Commit with meaningful messages
6. Push to your fork
7. Open a Pull Request

## Development Setup

### Prerequisites

Ensure you have the following installed:
- Node.js 18+
- Python 3.9+
- Go 1.20+
- Rust (latest stable)
- Relevant blockchain CLIs

### Installation

```bash
cd web3-examples

# Install dependencies for each project
cd solidity/erc20 && npm install
cd ../../typescript/wagmi-hooks && npm install
cd ../python/web3py-tools && pip install -r requirements.txt
# ... etc
```

## Commit Guidelines

We follow [Conventional Commits](https://www.conventionalcommits.org/):

```
<type>(<scope>): <description>

[optional body]

[optional footer]
```

### Types

- `feat`: New feature
- `fix`: Bug fix
- `docs`: Documentation changes
- `test`: Adding or updating tests
- `chore`: Maintenance tasks
- `refactor`: Code refactoring
- `perf`: Performance improvements
- `ci`: CI/CD changes

### Examples

```
feat(solidity): add ERC1155 multi-token standard
fix(python): resolve web3 connection timeout
docs(readme): update installation instructions
test(go): add unit tests for signature verifier
```

## Code Style

### Solidity
- Follow [Solidity Style Guide](https://docs.soliditylang.org/en/latest/style-guide.html)
- Use OpenZeppelin patterns
- Add NatSpec comments

### TypeScript/JavaScript
- Use ESLint and Prettier
- Follow Airbnb style guide
- Add JSDoc comments

### Python
- Follow PEP 8
- Use type hints
- Add docstrings

### Go
- Use `gofmt` and `golint`
- Follow official Go style
- Add package comments

### Rust
- Use `cargo fmt`
- Follow Rust API guidelines
- Add doc comments

## Testing

All code should include tests:

```bash
# Solidity
npx hardhat test

# TypeScript
npm test

# Python
pytest

# Go
go test ./...

# Rust
cargo test
```

## Documentation

- Every new feature needs documentation
- Update relevant README files
- Add code comments
- Include usage examples

## Pull Request Process

1. **Title**: Use conventional commit format
2. **Description**: Explain what and why
3. **Tests**: Ensure all tests pass
4. **Documentation**: Update as needed
5. **Review**: Address feedback promptly

### PR Checklist

- [ ] Code follows style guidelines
- [ ] Tests added/updated
- [ ] Documentation updated
- [ ] Commits follow conventional format
- [ ] CI passes
- [ ] No merge conflicts

## Adding New Language Examples

To add a new programming language:

1. Create directory: `web3-examples/<language>/`
2. Add example code with comments
3. Create comprehensive README.md
4. Add dependencies/package files
5. Include tests (if applicable)
6. Update main README.md
7. Add to CI workflow

## Security

- Never commit private keys or secrets
- Use `.env` files (gitignored)
- Follow security best practices
- Report vulnerabilities privately

## Questions?

- Open an issue for bugs
- Start a discussion for questions
- Join our Discord for chat

## License

By contributing, you agree that your contributions will be licensed under the same license as the project.

## Recognition

Contributors will be acknowledged in the project documentation.

Thank you for contributing! 🚀
