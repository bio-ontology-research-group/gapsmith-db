#!/usr/bin/env python3
"""Cursor-walk UniProtKB/Swiss-Prot for all EC-annotated reviewed
entries; emit a slim gzipped JSON snapshot suitable for the UniProt
existence verifier.

The built-in fetcher in gapsmith-db-ingest only fetches page 1 (the
step-based FetchPlan isn't a good fit for cursor pagination). This
standalone script does the walk and produces:

    data/uniprot/swissprot_ec.json  (gzipped)
        {"results": [{"primaryAccession": "...", "secondaryAccessions": [...]}, ...]}

Run after `gapsmith-db fetch --source uniprot` to replace the single-page
artefact. The sha256 must then be re-pinned in data/uniprot/SOURCE.toml.
"""

from __future__ import annotations

import argparse
import gzip
import json
import logging
import re
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


def iter_pages(query: str, size: int, max_retries: int = 5):
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
                with urllib.request.urlopen(req, timeout=60) as resp:
                    link_header = resp.headers.get("Link") or ""
                    body = resp.read()
                    data = json.loads(body)
                    entries = data.get("results") or []
                    LOG.info("page %d: %d entries", page, len(entries))
                    yield entries
                    m = _LINK_RE.search(link_header)
                    url = m.group(1) if m else None
                    break
            except (urllib.error.HTTPError, urllib.error.URLError) as e:
                if attempt >= max_retries:
                    raise
                sleep = min(2**attempt, 30)
                LOG.warning(
                    "page %d attempt %d failed: %s; retry in %ds", page, attempt, e, sleep
                )
                time.sleep(sleep)


def main(argv: list[str]) -> int:
    p = argparse.ArgumentParser(description=__doc__)
    p.add_argument("--out", default="data/uniprot/swissprot_ec.json")
    p.add_argument("--query", default=DEFAULT_QUERY)
    p.add_argument("--size", type=int, default=DEFAULT_SIZE)
    p.add_argument(
        "--max-pages", type=int, default=None, help="Stop after N pages (debug)."
    )
    args = p.parse_args(argv)

    logging.basicConfig(level=logging.INFO, format="%(asctime)s %(levelname)s %(message)s")
    all_entries: list[dict] = []
    for i, page in enumerate(iter_pages(args.query, args.size), 1):
        # Keep only the fields the UniProt existence verifier cares about.
        for e in page:
            slim = {"primaryAccession": e["primaryAccession"]}
            sec = e.get("secondaryAccessions")
            if sec:
                slim["secondaryAccessions"] = sec
            all_entries.append(slim)
        if args.max_pages and i >= args.max_pages:
            LOG.info("stopping after %d pages (--max-pages)", i)
            break
    LOG.info("total entries: %d", len(all_entries))

    payload = {"results": all_entries}
    body = json.dumps(payload).encode()
    with gzip.open(args.out, "wb", compresslevel=6) as f:
        f.write(body)
    import hashlib
    digest = hashlib.sha256()
    with open(args.out, "rb") as f:
        for chunk in iter(lambda: f.read(64 * 1024), b""):
            digest.update(chunk)
    LOG.info("wrote %s (%d entries, sha256=%s)", args.out, len(all_entries), digest.hexdigest())
    return 0


if __name__ == "__main__":
    sys.exit(main(sys.argv[1:]))
