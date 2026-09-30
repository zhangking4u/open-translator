# OpenTranslator Architecture


## 1. Overview

OpenTranslator is an open-source local AI translation platform.

The system aims to provide universal translation capability across different environments:

- Desktop applications
- Browsers
- Online meetings
- Mobile devices


The core design principle:

Local-first, modular, extensible.


---

# 2. High Level Architecture

             User

              |

    +-------------------+
    | Client Interfaces |
    +-------------------+

      |       |       |

  Desktop  Browser  Mobile


              |

              |

    Translation Core Service


              |

    +----------------+

    | Translation API |

    +----------------+

              |

              |

    Translation Engine Layer


      |          |          |

   HY-MT      NLLB       Qwen


              |

              |

      Model Runtime Layer


---

# 3. Core Components


## 3.1 Desktop Client

Responsibility:

- Capture selected text
- Display translation popup
- Communicate with Translation Core


Status:

Planned


---

## 3.2 Translation Core

Technology:

Rust


Responsibility:

- Provide translation API
- Manage translation workflow
- Coordinate translation engines


Current implementation:

translator-service


---

## 3.3 Translation API

Protocol:

REST API


Current endpoints:


GET /health

Purpose:

Check service availability.


POST /translate

Purpose:

Submit translation request.


Error responses:

JSON `{"error":{"kind","message"}}` with 400 invalid request, 500 internal, 502 engine unavailable, 504 timeout.


---

## 3.4 Translation Engine

The engine layer provides abstraction between business logic and AI models.


Design goal:

The core system should not depend on a specific model.


Possible implementations:

- Mock Engine
- HY-MT Engine
- NLLB Engine
- Qwen Engine


Current implementation:

MockEngine behind `engine::build`; every engine is wrapped in a timeout guard. The engine trait is async and fallible. Prompt building and language tag normalization live in the domain layer (`domain::prompt`, `domain::language`).


---

# 4. Design Principles


## Modularity

Each component should have clear responsibility.


## Model Independence

AI models can be replaced without changing upper layers.


## Local First

Translation should work without external services whenever possible.


## Cross Platform

Core logic should remain platform independent.


---

# 5. Current Architecture Status


Completed:

- Rust core service
- Axum API
- Basic REST endpoints
- Translation Engine abstraction (async trait + MockEngine)
- API layer split (`src/api`) with unit and integration tests


In Progress:

- Model adapter and local inference integration (Sprint 2)


Next:

- Engine selection and configuration
- Desktop client

