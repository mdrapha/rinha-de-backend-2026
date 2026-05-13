# Rinha de Backend 2026 — Detecção de Fraude com Busca Vetorial

Submissão para a [Rinha de Backend 2026](https://github.com/zanfranceschi/rinha-de-backend-2026).

**Stack**: Rust (axum) · VP-tree · f16 vectors · Nginx

## O Desafio

Construir uma API que recebe transações de cartão e decide se são fraude ou não, usando **busca vetorial** sobre um dataset de 3 milhões de referências rotuladas. Tudo isso rodando com no máximo **1 CPU e 350 MB de RAM**.

Para cada transação:
1. O payload é transformado em um vetor de **14 dimensões** (normalizado)
2. Os **5 vizinhos mais próximos** são buscados no dataset de referência
3. `fraud_score = fraudes entre os 5 / 5`
4. `approved = fraud_score < 0.6`

## Arquitetura

```
                ┌─────────────────────────────────┐
                │          Nginx (LB)             │
                │     porta 9999 · round-robin     │
                │     0.05 CPU · 10 MB             │
                └──────────┬──────────┬────────────┘
                           │          │
                ┌──────────▼──┐  ┌────▼──────────┐
                │   API 1     │  │   API 2       │
                │  port 8080  │  │  port 8080    │
                │ 0.475 CPU   │  │ 0.475 CPU     │
                │  170 MB     │  │  170 MB       │
                │             │  │               │
                │ ┌─────────┐ │  │ ┌─────────┐   │
                │ │ VP-tree │ │  │ │ VP-tree │   │
                │ │ 3M vecs │ │  │ │ 3M vecs │   │
                │ │  (f16)  │ │  │ │  (f16)  │   │
                │ └─────────┘ │  │ └─────────┘   │
                └─────────────┘  └───────────────┘
```

Cada instância da API carrega uma cópia completa do índice VP-tree em memória. Não há dependência entre as instâncias nem serviço externo de banco de dados — a busca vetorial é feita in-process, eliminando qualquer latência de rede.

**Recursos totais**: 1.0 CPU · 350 MB RAM

## Decisões Técnicas

### Por que Rust?

O scoring da Rinha premia fortemente o p99 baixo (escala logarítmica, cada 10x mais rápido = +1000 pontos). Rust oferece latência previsível sem garbage collector, controle fino de memória e auto-vetorização via LLVM.

### Por que VP-tree?

| Algoritmo    | Tipo       | Complexidade | Precisão | Memória extra |
|-------------|------------|-------------|----------|---------------|
| Brute Force | Exato      | O(N × 14)  | 100%     | 0             |
| **VP-tree** | **Exato**  | **O(log N)**| **100%** | **~12 MB**    |
| HNSW        | Aproximado | O(log N)    | ~95-99%  | ~100+ MB      |
| KD-tree     | Exato      | O(log N)*   | 100%     | ~24 MB        |

A VP-tree oferece busca **exata** em O(log N) no caso médio, com overhead de memória mínimo. Diferente de KD-trees, funciona bem em 14 dimensões. Diferente de HNSW, não sacrifica precisão — evitando falsos positivos/negativos por imprecisão do algoritmo.

### Por que f16?

O dataset tem 3 milhões de vetores × 14 dimensões. A memória por instância:

| Formato | Por vetor | 2 instâncias | Cabe em 350 MB? |
|---------|----------|-------------|-----------------|
| f32     | 56 bytes | 336 MB      | Não             |
| **f16** | **28 bytes** | **168 MB** | **Sim**     |

Usando half-precision (IEEE 754 f16), cada instância ocupa ~95 MB. Com `target-cpu=haswell`, as conversões f16↔f32 usam instruções F16C do hardware.

## Estrutura do Projeto

```
src/
├── main.rs          Entry point — CLI (preprocess / serve)
├── server.rs        Endpoints HTTP (axum): GET /ready, POST /fraud-score
├── types.rs         Structs de request/response (serde)
├── vectorize.rs     Payload → vetor de 14 dimensões (normalização + clamp)
├── distance.rs      Distância euclidiana f16×f16 e f32×f16
├── vptree.rs        VP-tree: construção (build) e busca KNN (query k=5)
└── dataset.rs       Pré-processamento do .gz e serialização do índice binário
```

### Fluxo de uma Requisição

```
POST /fraud-score
        │
        ▼
  Parse JSON (serde)           ~5 μs
        │
        ▼
  Vetorizar payload            ~1 μs
  (14 dims, normalização)
        │
        ▼
  VP-tree KNN (k=5)            ~100-500 μs
  (distância euclidiana,
   poda por triângulo)
        │
        ▼
  fraud_score = fraudes / 5
  approved = score < 0.6
        │
        ▼
  Resposta JSON                ~1 μs
```

### Pré-processamento (Build Time)

O índice VP-tree é construído durante o `docker build`, não no startup:

1. **Descomprime** `references.json.gz` (~284 MB JSON)
2. **Converte** 3M vetores de f64 para f16
3. **Constrói** a VP-tree reordenando os vetores por particionamento recursivo via mediana
4. **Serializa** em formato binário (~99 MB): vetores f16 + labels + medianas f32

No startup, o servidor apenas lê o arquivo binário direto para memória (~1s).

### VP-tree — Como Funciona

A VP-tree (Vantage Point Tree) particiona o espaço recursivamente:

1. Escolhe um **ponto de referência** (vantage point)
2. Calcula a distância de todos os outros pontos até ele
3. Divide pela **mediana**: metade mais próxima vai para a esquerda, metade mais distante vai para a direita
4. Repete recursivamente

Na busca, a **desigualdade triangular** permite podar subárvores inteiras:
- Se a query está a distância `d` do vantage point e o raio de busca é `τ`
- A subárvore esquerda (dist ≤ mediana) pode ser ignorada se `d - τ > mediana`
- A subárvore direita (dist > mediana) pode ser ignorada se `d + τ < mediana`

Com 3M vetores, a árvore tem profundidade ~21. Na prática, cada query visita apenas ~100-500 nós.

## Docker Build

O Dockerfile usa 3 estágios:

| Estágio        | O que faz                                            |
|---------------|------------------------------------------------------|
| `builder`      | Compila o binário Rust com `-C target-cpu=haswell`   |
| `preprocessor` | Baixa references.gz, constrói o índice VP-tree       |
| `runtime`      | Imagem mínima: binário + índice (~175 MB)            |

## Como Rodar Localmente

```bash
# Subir o stack completo
docker compose up -d

# Aguardar o carregamento do índice (~2s)
curl http://localhost:9999/ready

# Testar uma transação legítima
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
# → {"approved":true,"fraud_score":0.0}

# Parar
docker compose down
```

### Desenvolvimento sem Docker

```bash
# Baixar referências
mkdir -p resources
curl -L -o resources/references.json.gz \
  https://raw.githubusercontent.com/zanfranceschi/rinha-de-backend-2026/main/resources/references.json.gz

# Construir índice
cargo build --release
./target/release/rinha preprocess resources/references.json.gz /tmp/index.bin

# Rodar servidor
INDEX_PATH=/tmp/index.bin PORT=8080 ./target/release/rinha
```

## Testes

```bash
cargo test
```

12 testes cobrindo:
- Vetorização com exemplos da documentação oficial (transação legítima e fraudulenta)
- Cálculo de distância euclidiana (f16 e mixed f32×f16)
- Construção e consulta da VP-tree
- Serialização/deserialização do índice binário
- Cálculo de dia da semana (Sakamoto's algorithm)

## Licença

MIT
