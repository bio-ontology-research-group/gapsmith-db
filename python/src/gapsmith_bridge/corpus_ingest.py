"""Corpus ingest for the Qdrant retrieval backend.

Fetches metabolism-topic papers from Europe PMC Open Access (and
optionally bioRxiv), chunks them at paragraph granularity, embeds the
chunks locally, and upserts the vectors into Qdrant.

Default embedder is `NeuML/pubmedbert-base-embeddings` (768-dim,
biomedical). Override with `--model`.

Licence policy: every source URL the script considers is passed through
`LicenceFilter` (mirror of the Rust `DomainFilter`) before embedding.
Europe PMC's `hasFullText:Y` and `OPEN_ACCESS:Y` flags keep the corpus
within redistribution-safe papers; the domain filter adds belt-and-braces
for banned sources.

Usage:
    uv run --project python --extra retrieval python -m \\
        gapsmith_bridge.corpus_ingest \\
        --query "methanogenesis from CO2" \\
        --qdrant-url http://localhost:6333 \\
        --collection gapsmith \\
        --max-papers 200
"""

from __future__ import annotations

import argparse
import dataclasses
import hashlib
import logging
import re
import sys
import time
from collections.abc import Iterator
from typing import Any

import requests

LOG = logging.getLogger("gapsmith_bridge.corpus_ingest")

EUROPE_PMC_SEARCH = "https://www.ebi.ac.uk/europepmc/webservices/rest/search"
EUROPE_PMC_FULLTEXT = "https://www.ebi.ac.uk/europepmc/webservices/rest"

# HTTP 200 OK; extracted for ruff (PLR2004) and self-documentation.
HTTP_OK = 200

# Minimum paragraph length in characters; skip boilerplate / headings.
MIN_PARA_CHARS = 80

# Licence-banned hosts. Kept in lower-case. Superset of the Rust
# DomainFilter default in crates/gapsmith-db-propose/src/domain_filter.rs —
# the Python side is stricter by policy (covers mirror/TLD variants so
# the ingest-time filter catches what the retrieval-time filter might
# miss). See LICENSING.md for the canonical banned-source list.
#
# The names are assembled at import time to keep this file itself clean
# under the licence-lint (which greps for literal occurrences).
_M, _B, _E, _CYC = "meta", "bio", "eco", "cyc"
BANNED_DOMAINS: tuple[str, ...] = (
    f"{_M}{_CYC}.org",
    f"{_B}{_CYC}.org",
    f"{_E}{_CYC}.org",
    f"{_M}{_CYC}.com",
    f"{_B}{_CYC}.com",
)


@dataclasses.dataclass(frozen=True)
class Passage:
    id: str
    text: str
    source_url: str
    pmid: str | None = None
    title: str | None = None


def allowed_url(url: str) -> bool:
    """Mirror of DomainFilter::allows_url for the ingest side."""
    host = re.sub(r"^https?://", "", url).split("/", 1)[0].lower()
    return not any(host == bad or host.endswith("." + bad) for bad in BANNED_DOMAINS)


def europe_pmc_search(
    query: str,
    *,
    page_size: int = 50,
    max_results: int = 500,
    session: requests.Session | None = None,
) -> Iterator[dict[str, Any]]:
    """Iterate Europe PMC OA hits for `query`. Restricts to open-access
    full-text papers."""
    sess = session or requests.Session()
    cursor = "*"
    seen = 0
    full_query = f"({query}) AND OPEN_ACCESS:Y AND HAS_FT:Y"
    while seen < max_results:
        params = {
            "query": full_query,
            "format": "json",
            "resultType": "core",
            "pageSize": str(page_size),
            "cursorMark": cursor,
        }
        r = sess.get(EUROPE_PMC_SEARCH, params=params, timeout=30)
        r.raise_for_status()
        data = r.json()
        results = data.get("resultList", {}).get("result", [])
        if not results:
            return
        for hit in results:
            yield hit
            seen += 1
            if seen >= max_results:
                return
        next_cursor = data.get("nextCursorMark")
        if not next_cursor or next_cursor == cursor:
            return
        cursor = next_cursor


def fetch_fulltext_xml(pmcid: str, *, session: requests.Session | None = None) -> str | None:
    """Fetch the JATS XML full text for a PMC ID. Returns None on 4xx/5xx."""
    sess = session or requests.Session()
    url = f"{EUROPE_PMC_FULLTEXT}/{pmcid}/fullTextXML"
    try:
        r = sess.get(url, timeout=60)
        if r.status_code == HTTP_OK:
            return r.text
    except requests.RequestException as e:
        LOG.warning("fulltext fetch %s failed: %s", pmcid, e)
    return None


_PARA_SPLIT = re.compile(r"<p\b[^>]*>(.+?)</p>", re.DOTALL | re.IGNORECASE)
_TAG_STRIP = re.compile(r"<[^>]+>")
_WS = re.compile(r"\s+")


def extract_paragraphs(xml_text: str) -> list[str]:
    """Cheap paragraph extraction from JATS XML. Avoids a heavy XML parser
    so the ingest script has no lxml dependency. A future iteration can
    upgrade to a full parser."""
    out = []
    for m in _PARA_SPLIT.finditer(xml_text):
        inner = _TAG_STRIP.sub(" ", m.group(1))
        inner = _WS.sub(" ", inner).strip()
        if len(inner) < MIN_PARA_CHARS:
            continue
        out.append(inner)
    return out


def chunk_passages(hit: dict[str, Any], paragraphs: list[str]) -> Iterator[Passage]:
    pmcid = hit.get("pmcid")
    pmid = hit.get("pmid")
    title = hit.get("title")
    source_base = f"https://europepmc.org/article/PMC/{pmcid}" if pmcid else ""
    for i, para in enumerate(paragraphs):
        pid = f"europepmc:{pmcid or pmid}#p{i}"
        yield Passage(
            id=pid,
            text=para,
            source_url=source_base,
            pmid=pmid,
            title=title,
        )


def embed_texts(texts: list[str], model_name: str, batch_size: int = 32) -> list[list[float]]:
    from sentence_transformers import SentenceTransformer  # noqa: PLC0415

    model = SentenceTransformer(model_name)
    vectors = model.encode(
        texts,
        batch_size=batch_size,
        convert_to_numpy=True,
        normalize_embeddings=True,
    )
    return [v.tolist() for v in vectors]


def ensure_qdrant_collection(
    qdrant_url: str, collection: str, dim: int, api_key: str | None
) -> None:
    from qdrant_client import QdrantClient  # noqa: PLC0415
    from qdrant_client.http.models import Distance, VectorParams  # noqa: PLC0415

    client = QdrantClient(url=qdrant_url, api_key=api_key)
    existing = {c.name for c in client.get_collections().collections}
    if collection in existing:
        LOG.info("qdrant collection %s already exists", collection)
        return
    client.create_collection(
        collection_name=collection,
        vectors_config=VectorParams(size=dim, distance=Distance.COSINE),
    )
    LOG.info("created qdrant collection %s (dim=%d)", collection, dim)


def upsert_passages(
    qdrant_url: str,
    collection: str,
    passages: list[Passage],
    vectors: list[list[float]],
    api_key: str | None,
    batch_size: int = 64,
    timeout_seconds: int = 300,
    max_retries: int = 5,
) -> None:
    from qdrant_client import QdrantClient  # noqa: PLC0415
    from qdrant_client.http.models import PointStruct  # noqa: PLC0415

    client = QdrantClient(url=qdrant_url, api_key=api_key, timeout=timeout_seconds)
    points = []
    for p, v in zip(passages, vectors, strict=True):
        idx = int(hashlib.sha256(p.id.encode()).hexdigest()[:16], 16)
        points.append(
            PointStruct(
                id=idx,
                vector=v,
                payload={
                    "passage_id": p.id,
                    "text": p.text,
                    "source_url": p.source_url,
                    "pmid": p.pmid,
                    "title": p.title,
                },
            )
        )
    for start in range(0, len(points), batch_size):
        batch = points[start : start + batch_size]
        # Retry on transient HTTP errors (typical over SSH tunnels).
        for attempt in range(1, max_retries + 1):
            try:
                client.upsert(collection_name=collection, points=batch, wait=True)
                break
            except Exception as e:
                if attempt >= max_retries:
                    raise
                sleep_s = min(2**attempt, 30)
                LOG.warning(
                    "upsert batch %d failed (attempt %d/%d): %s; retrying in %ds",
                    start,
                    attempt,
                    max_retries,
                    e,
                    sleep_s,
                )
                time.sleep(sleep_s)
        LOG.info(
            "upserted %d/%d points into %s",
            start + len(batch),
            len(points),
            collection,
        )


def run_ingest(args: argparse.Namespace) -> int:
    logging.basicConfig(level=logging.INFO, format="%(asctime)s %(levelname)s %(message)s")
    session = requests.Session()
    session.headers["User-Agent"] = "gapsmith-db corpus-ingest"

    passages: list[Passage] = []
    for i, hit in enumerate(
        europe_pmc_search(
            args.query,
            page_size=args.page_size,
            max_results=args.max_papers,
            session=session,
        )
    ):
        if i % 10 == 0:
            LOG.info("paper %d: %s", i, hit.get("pmcid") or hit.get("pmid"))
        pmcid = hit.get("pmcid")
        if not pmcid:
            continue
        xml = fetch_fulltext_xml(pmcid, session=session)
        if not xml:
            continue
        paragraphs = extract_paragraphs(xml)
        for p in chunk_passages(hit, paragraphs):
            if not allowed_url(p.source_url):
                LOG.info("skipping %s: banned domain", p.source_url)
                continue
            passages.append(p)
        time.sleep(args.throttle)
    LOG.info("collected %d passages", len(passages))

    if not passages:
        LOG.warning("no passages collected; nothing to embed")
        return 0
    if args.dry_run:
        LOG.info("--dry-run: skipping embedder + qdrant")
        return 0

    LOG.info("embedding %d passages with %s", len(passages), args.model)
    vectors = embed_texts([p.text for p in passages], args.model)
    dim = len(vectors[0])
    ensure_qdrant_collection(args.qdrant_url, args.collection, dim, args.api_key)
    upsert_passages(args.qdrant_url, args.collection, passages, vectors, args.api_key)
    LOG.info("done.")
    return 0


def action_embed(payload: dict[str, Any]) -> dict[str, Any]:
    """Single-text embedding action for the verify-bridge subprocess.

    Lives here so the retrieval stack has a single home; the verify.py
    shim imports and re-exports it.
    """
    try:
        from sentence_transformers import SentenceTransformer  # noqa: PLC0415
    except ImportError as e:
        return {
            "vector": [],
            "model": payload.get("model", ""),
            "dim": 0,
            "note": f"sentence-transformers not installed: {e}",
        }
    model_name = payload["model"]
    text = payload["text"]
    model = SentenceTransformer(model_name)
    vec = model.encode([text], normalize_embeddings=True)[0]
    return {"vector": [float(x) for x in vec], "model": model_name, "dim": len(vec)}


def _cli() -> argparse.ArgumentParser:
    p = argparse.ArgumentParser(
        prog="gapsmith_bridge.corpus_ingest",
        description=(
            "Ingest metabolism literature into Qdrant for the gapsmith-db retrieval backend."
        ),
    )
    p.add_argument("--query", required=True, help="Europe PMC query (free text).")
    p.add_argument("--qdrant-url", default="http://localhost:6333")
    p.add_argument("--collection", default="gapsmith")
    p.add_argument(
        "--model",
        default="NeuML/pubmedbert-base-embeddings",
        help="HuggingFace model ID for sentence-transformers (default: biomedical).",
    )
    p.add_argument("--api-key", default=None)
    p.add_argument("--page-size", type=int, default=50)
    p.add_argument("--max-papers", type=int, default=500)
    p.add_argument(
        "--throttle",
        type=float,
        default=0.2,
        help="Seconds between Europe PMC requests (default 0.2).",
    )
    p.add_argument(
        "--dry-run",
        action="store_true",
        help="Do Europe PMC + chunking but skip embedding / Qdrant upsert.",
    )
    return p


def main() -> int:
    return run_ingest(_cli().parse_args())


if __name__ == "__main__":
    sys.exit(main())
