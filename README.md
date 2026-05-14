# Rinha de Backend 2026 — Detecção de Fraude com Busca Vetorial

Submissão para a [Rinha de Backend 2026](https://github.com/zanfranceschi/rinha-de-backend-2026).

**Stack**: Rust · monoio (io_uring) · IVF-Flat SIMD · Custom Load Balancer

## O Desafio

Construir uma API que recebe transações de cartão e decide se são fraude ou não, usando **busca vetorial** sobre um dataset de 3 milhões de referências rotuladas. Tudo isso rodando com no máximo **1 CPU e 350 MB de RAM**.

Para cada transação:
1. O payload é transformado em um vetor de **14 dimensões** (normalizado)
2. Os **5 vizinhos mais próximos** são buscados no dataset de referência
3. `fraud_score = fraudes entre os 5 / 5`
4. `approved = fraud_score < 0.6`

## Arquitetura

```
              ┌───────────────────────────────────┐
              │     Custom Proxy (io_uring)       │
              │   porta 9999 · round-robin FD     │
              │       0.1 CPU · 30 MB             │
              └──────────┬──────────┬─────────────┘
                    UDS FD pass    UDS FD pass
              ┌──────────▼──┐  ┌───▼────────────┐
              │   API 1     │  │   API 2        │
              │  monoio RT  │  │  monoio RT     │
              │ 0.45 CPU    │  │ 0.45 CPU       │
              │  160 MB     │  │  160 MB        │
              │             │  │                │
              │ ┌─────────┐ │  │ ┌─────────┐    │
              │ │IVF-Flat │ │  │ │IVF-Flat │    │
              │ │ K=4096  │ │  │ │ K=4096  │    │
              │ │i16 quant│ │  │ │i16 quant│    │
              │ │AVX2+FMA │ │  │ │AVX2+FMA │    │
              │ └─────────┘ │  │ └─────────┘    │
              └─────────────┘  └────────────────┘
```

O proxy aceita conexões TCP e repassa o **file descriptor** via Unix socket para as instâncias API, eliminando cópia de dados entre processos. Cada instância usa `monoio` (runtime io_uring) para I/O assíncrono de baixa latência.

**Recursos totais**: 1.0 CPU · 350 MB RAM

## Decisões Técnicas

### Por que IVF-Flat em vez de VP-tree?

| Algoritmo     | Complexidade  | Cache-friendly | SIMD | Memória   |
|--------------|--------------|----------------|------|-----------|
| VP-tree      | O(log N)     | Não (random)   | Não  | ~95 MB    |
| **IVF-Flat** | **O(N/K × P)** | **Sim (linear)** | **Sim** | **~90 MB** |
| HNSW         | O(log N)     | Não (random)   | Parcial | ~100+ MB |

A VP-tree, apesar de exata, sofre com acessos aleatórios de memória (cache misses) e não se beneficia de SIMD. O IVF-Flat organiza vetores em clusters contíguos na memória, permitindo scans lineares com instruções AVX2/FMA — processando **8 vetores simultaneamente** por iteração SIMD.

Com K=4096 centroids e NPROBE adaptativo (5 para casos claros, 24 para ambíguos), o IVF-Flat alcança latências consistentemente abaixo de 2ms mantendo detecção perfeita.

### Quantização i16

O dataset de 3M vetores × 14 dimensões precisa caber na memória com margem:

| Formato | Por vetor | Total (1 inst.) | Alinhamento SIMD |
|---------|----------|-----------------|------------------|
| f32     | 56 bytes | 168 MB          | Nativo           |
| f16     | 28 bytes | 84 MB           | Precisa converter|
| **i16** | **28 bytes** | **84 MB**   | **Nativo AVX2**  |

Os vetores são quantizados para i16 (escala 10000x) durante o build do índice. Isso permite operações SIMD diretamente em inteiros de 16 bits sem conversão, usando `_mm256_madd_epi16` para multiplicação e acumulação em uma instrução.

### monoio + io_uring

Em vez de tokio/epoll, usamos `monoio` — um runtime Rust baseado em io_uring. Vantagens no contexto da Rinha:
- **Completion-based I/O**: sem syscalls extras para poll
- **Batch submission**: múltiplas operações I/O submetidas de uma vez
- **Single-threaded**: sem overhead de sincronização entre threads

### Custom HTTP Parser

Sem frameworks HTTP (axum, hyper). O parser é manual com `memchr` para busca rápida de delimitadores. As respostas HTTP são **pré-computadas** como constantes estáticas — para cada possível `fraud_score` (0.0, 0.2, 0.4, 0.6, 0.8, 1.0), a resposta HTTP completa já está pronta em memória.

### FD Passing (Unix Domain Sockets)

O load balancer não faz proxy de bytes. Ele aceita a conexão TCP e passa o **raw file descriptor** para uma instância API via `sendmsg`/`recvmsg` com `SCM_RIGHTS`. A instância API lê e escreve diretamente no socket do cliente — zero-copy entre proxy e backend.

## Estrutura do Projeto

```
src/
├── main.rs          Entry point API — monoio runtime, UDS listener, FD handling
├── proxy.rs         Entry point Proxy — TCP accept, round-robin FD dispatch
├── build_index.rs   K-means clustering + quantização → index.bin.gz
├── data.rs          Carregamento do índice IVF embarcado (include_bytes!)
├── search.rs        IVF-Flat KNN com AVX2/FMA intrinsics
├── feature.rs       Payload → vetor 14D (normalização + clamp)
├── parse.rs         Parser JSON manual (zero-alloc, sem serde no hot path)
├── http.rs          HTTP parser + connection handler (memchr, writev)
├── reply.rs         Respostas HTTP pré-computadas
├── config.rs        Configuração via env vars
└── socket.rs        FD passing via Unix sockets (SCM_RIGHTS)
```

### Fluxo de uma Requisição

```
TCP connect (porta 9999)
        │
        ▼
  Proxy: accept + sendmsg(fd)     ~10 μs
        │ (UDS FD pass)
        ▼
  API: recvmsg(fd) → TcpStream
        │
        ▼
  HTTP parse (memchr)              ~1 μs
        │
        ▼
  JSON parse (manual)              ~2 μs
        │
        ▼
  Vectorize (14D)                  ~1 μs
        │
        ▼
  IVF search (AVX2/FMA)           ~50-200 μs
  ├─ centroid distances
  ├─ top-N probe selection
  └─ SIMD block scan (i16)
        │
        ▼
  Resposta pré-computada           ~0 μs
  (writev direto no socket)
```

### Build do Índice (Compile Time)

O índice IVF é construído durante o `docker build` e embarcado no binário:

1. **Carrega** `references.json.gz` (~47 MB gz → 3M vetores)
2. **K-means++** com K=4096 clusters, 25 iterações de Lloyd
3. **Quantiza** vetores para i16 (escala 10000x)
4. **Organiza** em blocos de 8 vetores (alinhados para SIMD)
5. **Comprime** com gzip → `data/index.bin.gz` (~30 MB)
6. **Embarca** via `include_bytes!` no binário final

No startup, o servidor descomprime o índice em ~200ms e faz warmup de 500 queries aleatórias para aquecer caches.

## Benchmark Local (k6, mesmo script do desafio)

Resultados em WSL2 (não representam o hardware do desafio):

| Métrica | Valor |
|---|---|
| p99 | 2.09ms |
| HTTP errors | 0 |
| False Positives | 0 |
| False Negatives | 0 |
| Score p99 | 2,680.85 |
| Score detecção | 3,000.00 (máximo) |
| **Score FINAL** | **5,680.85** |

## Como Rodar Localmente

```bash
docker compose up -d

curl http://localhost:9999/ready

curl -X POST http://localhost:9999/fraud-score \
  -H "Content-Type: application/json" \
  -d '{
    "id": "tx-1",
    "transaction": {"amount": 41.12, "installments": 2, "requested_at": "2026-03-11T18:45:53Z"},
    "customer": {"avg_amount": 82.24, "tx_count_24h": 3, "known_merchants": ["MERC-003", "MERC-016"]},
    "merchant": {"id": "MERC-016", "mcc": "5411", "avg_amount": 60.25},
    "terminal": {"is_online": false, "card_present": true, "km_from_home": 29.23},
    "last_transaction": null
  }'

docker compose down
```

## Licença

MIT
