# ADR 0007 · Derive GitHub's API origin from the configured web URL

**Status** Accepted

A GitHub connection has one required URL field. It takes the web URL a person uses, for example
`https://github.com`. The token is sealed against that URL's origin using
[ADR 0006](0006-secure-secrets.md).

| Configured web URL           | REST base                              | GraphQL endpoint                            |
| ---------------------------- | -------------------------------------- | ------------------------------------------- |
| `https://github.com`         | `https://api.github.com`               | `https://api.github.com/graphql`            |
| `https://<tenant>.ghe.com`   | `https://api.<tenant>.ghe.com`         | `https://api.<tenant>.ghe.com/graphql`      |
| Another HTTP or HTTPS origin | Configured URL with `/api/v3` appended | Configured URL with `/api/graphql` appended |

The cloud forms use HTTPS and its standard port and ignore any path on the web URL. Enterprise
Server preserves the configured scheme, port, and context path. Queries and fragments are dropped
before building request paths. Addresses with embedded credentials are refused before the client
is made.

The derivation runs once when the API client is built. The client sends the bearer token only to
the resulting API origin. A redirect to a different scheme, host, or port stops there. The
configured web origin remains the binding for the secret because it is the origin the person
selected. A change to that origin requires entering the token again.

[GitHub documents these GraphQL endpoints](https://docs.github.com/en/enterprise-cloud%40latest/graphql/guides/managing-enterprise-accounts#3-setting-up-insomnia-to-use-the-github-graphql-api-with-enterprise-accounts).
