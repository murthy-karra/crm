#!/usr/bin/env python3
"""Seeds two Organizations, each with an admin and a member, entirely
through the live HTTP API (docs/specs/SLICE_004.md §4, §11) -- the same
create/invite/accept sequence a real platform admin and org admin would
drive by hand. Run after `crm-admin bootstrap-platform-admin`, which
remains the one non-API step (there is no route to grant platform admin;
D-021 names migrations and that one bootstrap command as the sanctioned
exceptions to "no direct database writes").

Mirrors scripts/demo-leads' security discipline: the seed password comes
only from CRM_DEV_SEED_PASSWORD and is never placed on argv or printed.
Session cookies are held as plain strings, one per actor, and replayed by
hand rather than through requests' automatic cookie jar: with
CRM_SESSION_COOKIE_SECURE=true the cookie is Secure, and unlike curl 8.7
(which special-cases loopback -- see scripts/demo-leads), requests' jar
will not resend a Secure cookie over plain http://, even to 127.0.0.1.
"""
import os
import sys

try:
    import requests
except ImportError:
    sys.exit("the 'requests' package is required: pip install -r scripts/requirements.txt")

API_URL = os.environ.get("CRM_DEMO_API_URL", "http://127.0.0.1:3000")
PLATFORM_ADMIN_EMAIL = "owner@platform.test"

SEED_ORGS = [
    {
        "name": "Acme Realty",
        "admin": {"email": "alice@acme.test", "display_name": "Alice Anderson"},
        "member": {"email": "carol@acme.test", "display_name": "Carol Chen"},
    },
    {
        "name": "Best Realty",
        "admin": {"email": "bob@best.test", "display_name": "Bob Baker"},
        "member": {"email": "dave@best.test", "display_name": "Dave Diaz"},
    },
]


class Actor:
    """One logged-in user, identified purely by its session cookie string."""

    def __init__(self):
        self._cookie = None

    def post(self, path, body):
        headers = {"Cookie": self._cookie} if self._cookie else {}
        resp = requests.post(f"{API_URL}{path}", json=body, headers=headers)
        if not resp.ok:
            sys.exit(f"POST {path} -> {resp.status_code}: {resp.text}")
        set_cookie = resp.headers.get("set-cookie")
        if set_cookie:
            self._cookie = set_cookie.split(";", 1)[0]
        return resp


def _token_from_accept_path(accept_path):
    return accept_path.rsplit("/", 1)[-1]


def login(actor, email, password):
    actor.post("/api/session", {"email": email, "password": password})


def create_organization(actor, name):
    resp = actor.post("/api/platform/organizations", {"name": name})
    return resp.json()["organization"]["id"]


def invite_org_admin(actor, organization_id, email):
    resp = actor.post(
        f"/api/platform/organizations/{organization_id}/invitations",
        {"email": email, "role": "admin"},
    )
    return _token_from_accept_path(resp.json()["accept_path"])


def invite_member(actor, email):
    resp = actor.post("/api/organization/invitations", {"email": email, "role": "member"})
    return _token_from_accept_path(resp.json()["accept_path"])


def accept_invitation(actor, token, display_name, password):
    # Ignores any cookie already on `actor` and sets a fresh one -- the
    # same rule the route itself follows (routes/invitations.rs).
    actor.post(
        "/api/invitations/accept",
        {"token": token, "display_name": display_name, "password": password},
    )


def main():
    password = os.environ.get("CRM_DEV_SEED_PASSWORD")
    if not password:
        sys.exit("CRM_DEV_SEED_PASSWORD is not set")

    platform = Actor()
    login(platform, PLATFORM_ADMIN_EMAIL, password)

    for org in SEED_ORGS:
        organization_id = create_organization(platform, org["name"])

        admin_token = invite_org_admin(platform, organization_id, org["admin"]["email"])
        admin = Actor()
        accept_invitation(admin, admin_token, org["admin"]["display_name"], password)

        member_token = invite_member(admin, org["member"]["email"])
        member = Actor()
        accept_invitation(member, member_token, org["member"]["display_name"], password)

        print(f"seeded {org['name']}: {org['admin']['email']} (admin), {org['member']['email']} (member)")


if __name__ == "__main__":
    try:
        main()
    except requests.exceptions.RequestException as err:
        sys.exit(f"could not reach the API at {API_URL}: {err}")
