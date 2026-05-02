# Terraform graph fixture — Vercel-style project + Supabase Postgres

Use this directory as `terraform init` / `terraform graph` cwd when testing the standalone example.

```bash
cd crates/graph_view/examples/fixtures/vercel_supabase_stack
terraform init
```

Then from the repo root:

```bash
cargo run -p graph_view --example standalone_graph --no-default-features
```

Resources are **stubbed** with `null_resource` and explicit `depends_on` so the dependency graph has multiple layers without calling real APIs.
