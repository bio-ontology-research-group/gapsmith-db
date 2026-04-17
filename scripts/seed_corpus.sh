#!/usr/bin/env bash
# Seed the Qdrant retrieval corpus with Europe PMC OA papers across the
# breadth of pathways in proposals/catalogue/. Runs sequentially so
# Europe PMC's rate limits stay happy. Idempotent: re-running upserts
# the same points (sha256-derived IDs), so the collection is
# de-duplicated across queries.
set -euo pipefail

QDRANT_URL="${QDRANT_URL:-http://localhost:6333}"
COLLECTION="${COLLECTION:-gapsmith}"
MAX_PAPERS="${MAX_PAPERS:-150}"
# Resume from 1-based index; skip earlier queries (use after a partial run).
SEED_START="${SEED_START:-1}"

queries=(
  "central carbon metabolism glycolysis TCA pentose phosphate Entner-Doudoroff"
  "methanogenesis Wood-Ljungdahl acetogenesis carbon fixation rTCA 3-hydroxypropionate"
  "nitrogen fixation denitrification nitrification anammox assimilation"
  "sulfate reduction sulfur oxidation dissimilatory assimilatory bacteria"
  "cofactor biosynthesis vitamin coenzyme heme cobalamin F420 CoA NAD riboflavin"
  "amino acid biosynthesis degradation bacteria catabolism shikimate"
  "anaerobic respiration fermentation butyric propionic lactic mixed-acid"
  "secondary metabolism NRPS polyketide antibiotic biosynthesis siderophore"
)

log_dir="proposals/runs"
mkdir -p "${log_dir}"
log="${log_dir}/corpus_seed_$(date -u +%Y%m%dT%H%M%SZ).log"
echo "seed log: ${log}"

idx=0
for q in "${queries[@]}"; do
  idx=$((idx + 1))
  if [ "${idx}" -lt "${SEED_START}" ]; then
    echo "=== skip ${idx}: ${q}" | tee -a "${log}"
    continue
  fi
  echo "=== query: ${q}" | tee -a "${log}"
  uv run --project python --extra retrieval --quiet python -m gapsmith_bridge.corpus_ingest \
    --query "${q}" \
    --qdrant-url "${QDRANT_URL}" \
    --collection "${COLLECTION}" \
    --max-papers "${MAX_PAPERS}" \
    2>&1 | tee -a "${log}"
done

echo "=== collection summary:" | tee -a "${log}"
curl -s "${QDRANT_URL}/collections/${COLLECTION}" | tee -a "${log}"
echo | tee -a "${log}"
