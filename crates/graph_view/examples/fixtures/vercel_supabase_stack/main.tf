terraform {
  required_providers {
    null = {
      source  = "hashicorp/null"
      version = "~> 3.2"
    }
  }
}

# --- Providers (graph nodes) ---
resource "null_resource" "vercel_project_config" {}

resource "null_resource" "supabase_project_config" {}

resource "null_resource" "database_schema_migrations" {
  depends_on = [null_resource.supabase_project_config]
}

# --- App deployment pipeline ---
resource "null_resource" "frontend_build" {
  depends_on = [null_resource.vercel_project_config]
}

resource "null_resource" "frontend_deploy_preview" {
  depends_on = [
    null_resource.vercel_project_config,
    null_resource.frontend_build,
  ]
}

resource "null_resource" "frontend_deploy_production" {
  depends_on = [
    null_resource.vercel_project_config,
    null_resource.frontend_build,
    null_resource.database_schema_migrations,
  ]
}

# --- API / edge ---
resource "null_resource" "serverless_api_bundle" {
  depends_on = [
    null_resource.vercel_project_config,
    null_resource.supabase_project_config,
  ]
}

resource "null_resource" "edge_functions_deploy" {
  depends_on = [
    null_resource.vercel_project_config,
    null_resource.serverless_api_bundle,
  ]
}

# --- Background workers ---
resource "null_resource" "background_worker_image" {
  depends_on = [null_resource.supabase_project_config]
}

resource "null_resource" "background_worker_deploy" {
  depends_on = [
    null_resource.supabase_project_config,
    null_resource.background_worker_image,
    null_resource.database_schema_migrations,
  ]
}

# --- Secrets / env wiring ---
resource "null_resource" "sync_vercel_env_from_supabase" {
  depends_on = [
    null_resource.vercel_project_config,
    null_resource.supabase_project_config,
  ]
}

resource "null_resource" "final_integration_check" {
  depends_on = [
    null_resource.frontend_deploy_production,
    null_resource.edge_functions_deploy,
    null_resource.background_worker_deploy,
    null_resource.sync_vercel_env_from_supabase,
  ]
}
