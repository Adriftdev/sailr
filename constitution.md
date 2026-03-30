# Sailr Constitution

This document defines the governing principles, engineering guidelines, and contributor expectations for the Sailr project. It serves as the ultimate source of truth for architectural decisions and project culture.

## Engineering Philosophy

Our engineering approach is driven by the pursuit of uncompromising quality, where both high performance and rigorous safety are non-negotiable defaults. 

* **Performance and Safety by Default:** Core tooling, systems software, and backend logic must prioritize memory safety, fearless concurrency, and zero-cost abstractions. We do not compromise on security or efficiency.
* **Clarity and Conciseness:** Code and documentation must be detailed and comprehensive when dealing with complex concepts, but strictly concise everywhere else. Avoid unnecessary verbosity, boilerplate, or repetition.
* **Idiomatic and Predictable Design:** Follow community-standard linting, enforce strict type checking, and use conventional commit structures. Maintainability is fundamental to our success.

## Technology Stack Guidelines

We practice pragmatic technology selection. We use the exact right tool for the specific domain, maintaining strict boundaries between different components of our architecture.

* **Systems, Tooling, and Heavy Backend:** Prioritize uncompromising safety and performance architectures. 
  * *Primary Technology:* **Rust**
* **Lightweight Backend Services:** Emphasize simple, highly concurrent architectures for services where the maximum control of a systems language is overkill.
  * *Primary Technology:* **Go**
* **Frontend:** Maintain strict isolation from backend logic, favoring fine-grained reactivity and minimal runtime overhead.
  * *Primary Technology:* **SolidJS / TypeScript**
* **Machine Learning:** Default to standard Python-based ecosystems out of necessity, but actively seek out and transition to safer, compiled alternatives whenever viable.

## Contributor Expectations

Our collaboration model is built on directness, factual accuracy, and a shared commitment to excellence.

* **Direct and Candid Collaboration:** Code reviews and discussions must be factual, straightforward, and grounded in reality.
* **Constructive Correction:** Correct misconceptions gently but firmly. Our highest priority is maintaining the project's engineering standards.
* **Focus on the Code:** Keep discussions centered on technical merit, architecture, and alignment with this constitution.

## Amendments

### Amendment I: Uncompromising Error Handling and Data Integrity
* **Zero-Panic Policy:** Use of `.unwrap()` or `.expect()` is strictly forbidden in core domain logic, serialization, and deserialization routines. All fallible operations must propagate errors via `Result` or safely resolve to `None`. Panics are exclusively reserved for unrecoverable state corruption, never for unexpected input.
* **Symmetrical Data Structures:** Any data model that interacts with external inputs (e.g., config parsing) must be predictably and safely serializable and deserializable. Silent dropping of valid data or state during parsing is considered a critical defect.
* **Truth in Documentation:** Inline comments must strictly reflect the code's factual reality. Stale comments or misleading inline documentation that contradicts actual code behavior (especially within tests and domain rules) are worse than absent documentation and must be corrected as bugs.
