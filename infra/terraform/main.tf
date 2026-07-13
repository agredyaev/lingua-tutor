# Lingua Tutor infrastructure (11-infrastructure.md §1-2).
# Terraform owns: D1 databases, Access application + policy.
# Wrangler owns: Worker deploys, migrations, secrets (§1 split).

terraform {
  required_version = ">= 1.9"
  required_providers {
    cloudflare = {
      source  = "cloudflare/cloudflare"
      version = "~> 5.0"
    }
  }
}

provider "cloudflare" {
  # CLOUDFLARE_API_TOKEN from the environment.
}

variable "account_id" {
  type        = string
  description = "Cloudflare account ID"
}

variable "owner_email" {
  type        = string
  description = "Single owner allowed through Cloudflare Access (10-security.md)"
}

variable "workers_subdomain" {
  type        = string
  description = "workers.dev subdomain of the account"
}

# --- D1 (04-database.md; forward-only migrations applied via wrangler) ---

resource "cloudflare_d1_database" "dev" {
  account_id = var.account_id
  name       = "lingua-tutor-dev"
}

resource "cloudflare_d1_database" "prod" {
  account_id = var.account_id
  name       = "lingua-tutor-prod"

  # Learning history is the product; replacement requires a deliberate,
  # reviewed change (11-infrastructure.md §2 D1 lifecycle protection).
  lifecycle {
    prevent_destroy = true
  }
}

# --- Cloudflare Access: single-owner gate on the production Worker ---
# (10-security.md §2; JWT verified again inside the Worker at 10.2.)

# v5 provider: policies are standalone and attached via the application's
# `policies` list.
resource "cloudflare_zero_trust_access_policy" "owner_only" {
  account_id = var.account_id
  name       = "owner-only"
  decision   = "allow"

  include = [{
    email = { email = var.owner_email }
  }]
}

resource "cloudflare_zero_trust_access_application" "api" {
  account_id                = var.account_id
  name                      = "lingua-tutor"
  domain                    = "lingua-tutor-api.${var.workers_subdomain}.workers.dev"
  type                      = "self_hosted"
  session_duration          = "168h"
  auto_redirect_to_identity = true

  policies = [{
    id         = cloudflare_zero_trust_access_policy.owner_only.id
    precedence = 1
  }]
}

output "d1_dev_id" {
  value = cloudflare_d1_database.dev.id
}

output "d1_prod_id" {
  value = cloudflare_d1_database.prod.id
}

output "access_aud" {
  description = "Access application AUD tag — Worker validates JWTs against it"
  value       = cloudflare_zero_trust_access_application.api.aud
}
