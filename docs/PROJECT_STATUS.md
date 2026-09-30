# OpenTranslator Project Status


## 1. Project Overview

Project Name:

OpenTranslator


Vision:

Build an open-source, local AI translation infrastructure.

The goal is to provide universal translation capability across desktop applications, browsers, meetings and mobile devices.

The system should be:

- Local-first
- Privacy-preserving
- Free and open-source
- Extensible


---

## 2. Product Direction

MVP Goal:

Desktop selection translation.


Primary user scenario:

A user selects English text anywhere on the desktop and immediately receives AI-powered translation.


Example:

Input:

kernel panic


Output:

内核崩溃

Explanation:

Linux terminology describing a fatal kernel error state.


---

## 3. Development Environment


Operating System:

Ubuntu 26.04.1 LTS


Hardware:

CPU:
Intel Core i5-14400


Memory:

32GB RAM


GPU:

Intel UHD Graphics 730


Architecture:

x86_64


Model Runtime:

Ollama v0.35.0 (user-space install at `~/.local/opt/ollama`)


Current model:

qwen2.5:7b (GGUF, ~4.7 GB, CPU inference)


---

## 4. Technology Stack


Core Service:

Rust


Web Framework:

Axum


API:

REST API


Planned Database:

SQLite


Model Runtime:

Ollama (local, CPU inference)


Current model:

qwen2.5:7b


Candidate models:

- HY-MT
- NLLB
- Qwen Translation Models


---

## 5. Repository Structure


open-translator/

├── core/

│   └── translator-service/

│

├── desktop/

│

├── models/

│

├── docs/

│

└── tests/


---

## 6. Completed Work


### Environment

Completed:

- Ubuntu development environment
- Git installation
- Rust toolchain installation


### Core Service

Completed:

- Rust project created
- Axum HTTP server
- Health endpoint
- Translation endpoint (mock and Ollama engines)


Current APIs:


GET:

/health


POST:

/translate


Example:

Input:

hello world


Current output:

[Mock Translation] hello world


---

## 7. Git Status


Current branch:

main


Latest commit:

docs: record ollama adapter and live verification


---

## 8. Current Development Phase


Sprint:

Sprint 2 (preparation)


Goal:

Integrate a local translation model.


---

## 9. Next Steps


1. Evaluate dedicated MT models (Hunyuan-MT) against qwen2.5:7b

2. Prepare desktop client integration

3. Plan browser integration


---

## 10. Development Principles


Follow:

- Clean Architecture
- Domain Driven Design concepts
- API-first design
- Modular architecture
- Open-source engineering practices


Avoid:

- Premature complexity
- Tight coupling with a specific model
- Platform-specific implementation in core layer


---

## Sprint 1.2 Progress


Completed:

- Created translation domain model
- Added TranslationEngine trait (async, dyn-compatible)
- Added MockTranslationEngine
- Integrated API with engine layer
- Split API layer into `src/api`
- Added library target (`src/lib.rs`) with unit and integration tests


Current runtime flow:


HTTP API

↓

Translation Domain

↓

TranslationEngine

↓

MockEngine

↓

TranslationResult


Status:

Translation architecture abstraction completed; API layer split and test foundation in place.


## Sprint 2 Preparation Progress


Completed:

- TranslationEngine now returns `Result` with a shared `TranslationError`
- API maps errors to JSON `{"error":{"kind","message"}}` (400/500/502/504)
- Env-based config: `TRANSLATOR_BIND_ADDR`, `TRANSLATOR_ENGINE`, `TRANSLATOR_TIMEOUT_MS`; engine factory (`engine::build`)
- Timeout guard (`TimeoutEngine`) wrapping every engine; expiry maps to 504
- Language tag normalization (`domain::language`) and MT prompt builder (`domain::prompt`)
- Ollama engine adapter and model config (`TRANSLATOR_MODEL_URL`, `TRANSLATOR_MODEL`)
- Live end-to-end translation via local Ollama (qwen2.5:7b, CPU)
- Tests for invalid input, engine failure, timeout, config parsing, prompt building and Ollama adapter


Pending:

- Model quality evaluation (dedicated MT models such as Hunyuan-MT)
- Desktop client preparation
