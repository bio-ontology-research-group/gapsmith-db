#!/usr/bin/env python3
"""Cursor-walk UniProtKB/Swiss-Prot for all reviewed entries; emit a
slim gzipped JSON snapshot suitable for the UniProt existence verifier.

The built-in fetcher in gapsmith-db-ingest only fetches page 1 (the
step-based FetchPlan isn't a good fit for cursor pagination). This
standalone script does the walk and produces:

    data/uniprot/swissprot_ec.json  (gzipped)
        {"results": [{"primaryAccession": "...", "secondaryAccessions": [...]}, ...]}

The fetch is checkpointed every `--checkpoint-every` pages to
`<out>.ckpt.json`; re-running picks up from the last checkpoint so a
hung connection mid-walk doesn't cost the whole run.

Run after `gapsmith-db fetch --source uniprot` to replace the single-page
artefact. The sha256 must then be re-pinned in data/uniprot/SOURCE.toml.
"""

from __future__ import annotations

import argparse
import gzip
import hashlib
import http.client
import json
import logging
import os
import re
import socket
import sys
import time
import urllib.error
import urllib.parse
import urllib.request

# All reviewed Swiss-Prot entries. A tighter `ec:*` filter misses enzyme
# subunits that lack a recommendedName EC (the EC number is typically
# on the catalytic subunit only, not each complex member). For an
# "accession exists" check we need the full reviewed set.
DEFAULT_QUERY = "reviewed:true"
DEFAULT_SIZE = 500
# `fields=accession` alone skips secondaryAccessions, so use the default
# response and keep only what the verifier needs during post-processing.
BASE = "https://rest.uniprot.org/uniprotkb/search"

LOG = logging.getLogger("fetch_uniprot_snapshot")

_LINK_RE = re.compile(r'<([^>]+)>;\s*rel="next"', re.IGNORECASE)

# Exceptions that indicate a transient network hiccup; all are retried
# with exponential backoff up to `max_retries`.
TRANSIENT_EXC = (
    urllib.error.HTTPError,
    urllib.error.URLError,
    http.client.HTTPException,
    ConnectionError,
    TimeoutError,
    socket.timeout,
    OSError,
)


def iter_pages(query: str, size: int, max_retries: int = 8, start_url: str | None = None):
    """Yield (url, entries) per page.

    Yielding the URL too lets the caller checkpoint per-page.
    """
    if start_url:
        url = start_url
    else:
        params = f"query={urllib.parse.quote(query)}&format=json&size={size}"
        url = f"{BASE}?{params}"
    page = 0
    while url:
        page += 1
        attempt = 0
        while True:
            attempt += 1
            req = urllib.request.Request(
                url,
                headers={"User-Agent": "gapsmith-db/fetch-uniprot-snapshot"},
            )
            try:
                with urllib.request.urlopen(req, timeout=120) as resp:
                    link_header = resp.headers.get("Link") or ""
                    body = resp.read()
                    data = json.loads(body)
                    entries = data.get("results") or []
                    LOG.info("page %d: %d entries", page, len(entries))
                    yield url, entries
                    m = _LINK_RE.search(link_header)
                    url = m.group(1) if m else None
                    break
            except TRANSIENT_EXC as e:
                if attempt >= max_retries:
                    raise
                sleep = min(2**attempt, 60)
                LOG.warning(
                    "page %d attempt %d failed: %s; retry in %ds",
                    page,
                    attempt,
                    e,
                    sleep,
                )
                time.sleep(sleep)


def _load_checkpoint(path: str) -> tuple[list[dict], str | None]:
    if not os.path.exists(path):
        return [], None
    try:
        with open(path) as f:
            ckpt = json.load(f)
        entries = ckpt.get("entries", [])
        start_url = ckpt.get("next_url")
        LOG.info(
            "resuming from checkpoint: %d entries, cursor=%s",
            len(entries),
            "<present>" if start_url else "<none>",
        )
        return entries, start_url
    except (json.JSONDecodeError, OSError, KeyError) as e:
        LOG.warning("checkpoint at %s unreadable: %s; starting fresh", path, e)
        return [], None


def _write_checkpoint(path: str, entries: list[dict], next_url: str | None) -> None:
    tmp = path + ".tmp"
    with open(tmp, "w") as f:
        json.dump({"entries": entries, "next_url": next_url}, f)
    os.replace(tmp, path)


def _slim(entry: dict) -> dict:
    """Keep only the fields the UniProt existence verifier needs."""
    out = {"primaryAccession": entry["primaryAccession"]}
    sec = entry.get("secondaryAccessions")
    if sec:
        out["secondaryAccessions"] = sec
    return out


def main(argv: list[str]) -> int:
    p = argparse.ArgumentParser(description=__doc__)
    p.add_argument("--out", default="data/uniprot/swissprot_ec.json")
    p.add_argument("--query", default=DEFAULT_QUERY)
    p.add_argument("--size", type=int, default=DEFAULT_SIZE)
    p.add_argument("--max-pages", type=int, default=None, help="Stop after N pages (debug).")
    p.add_argument(
        "--checkpoint-every",
        type=int,
        default=50,
        help="Persist partial state every N pages so a crash doesn't lose hours of work.",
    )
    args = p.parse_args(argv)

    logging.basicConfig(level=logging.INFO, format="%(asctime)s %(levelname)s %(message)s")

    ckpt_path = args.out + ".ckpt.json"
    all_entries, start_url = _load_checkpoint(ckpt_path)

    try:
        for i, (url_used, page) in enumerate(
            iter_pages(args.query, args.size, start_url=start_url), 1
        ):
            all_entries.extend(_slim(e) for e in page)
            if i % args.checkpoint_every == 0:
                _write_checkpoint(ckpt_path, all_entries, url_used)
                LOG.info("checkpoint: %d pages, %d entries", i, len(all_entries))
            if args.max_pages and i >= args.max_pages:
                LOG.info("stopping after %d pages (--max-pages)", i)
                break
    except Exception:
        # Write a final checkpoint so we can resume — iter_pages
        # exhausted retries, so we keep what we have and surface.
        _write_checkpoint(ckpt_path, all_entries, None)
        raise

    LOG.info("total entries: %d", len(all_entries))

    payload = {"results": all_entries}
    body = json.dumps(payload).encode()
    with gzip.open(args.out, "wb", compresslevel=6) as f:
        f.write(body)
    digest = hashlib.sha256()
    with open(args.out, "rb") as f:
        for chunk in iter(lambda: f.read(64 * 1024), b""):
            digest.update(chunk)
    LOG.info(
        "wrote %s (%d entries, sha256=%s)",
        args.out,
        len(all_entries),
        digest.hexdigest(),
    )
    # Clean up the checkpoint on successful completion.
    if os.path.exists(ckpt_path):
        os.remove(ckpt_path)
    return 0


if __name__ == "__main__":
    sys.exit(main(sys.argv[1:]))
