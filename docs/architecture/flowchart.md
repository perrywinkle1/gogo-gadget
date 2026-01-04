# GoGoGadget Architecture Flowcharts

These flowcharts are derived from the current code paths in `src/` and are meant to be read top-down.
Each chart includes a short explanation so the intent is clear without cross-referencing code.

## 1) System Overview (Single vs Workers)

Explains how the CLI selects an execution mode, runs the loop, and verifies completion.

```mermaid
flowchart TB
    subgraph CLI[CLI Entry]
        Args[Parse args]
        Analyze[TaskAnalyzer]
        Mode{Execution mode?}
    end

    subgraph Single[Single-Agent Loop]
        TL[TaskLoop]
        Verify[Verifier]
        Signals[.gogo-gadget-* signals]
    end

    subgraph Workers[Worker Loop]
        Coord[Coordinator]
        Decompose[TaskDecomposer]
        Execute[WorkerExecutor]
        Aggregate[Result aggregator]
        VerifyWorkers[Verifier]
    end

    Args --> Analyze --> Mode
    Mode -->|Single| TL
    Mode -->|Workers| Coord

    TL --> Verify --> Signals

    Coord --> Decompose --> Execute --> Aggregate --> VerifyWorkers
    VerifyWorkers --> Signals
```

## 2) Iterative Task Loop (Verification First)

Shows the Ralph-style loop that runs until evidence-backed completion is reached.
Anti-laziness checks are integrated into the verification step.

```mermaid
flowchart LR
    Start[Start task] --> Iteration[Run iteration]
    Iteration --> Output[Collect model output + artifacts]
    Output --> Verify[Build/test + LLM completion check]
    Verify --> Evidence{Evidence sufficient?}
    Evidence -->|No| Feedback[Generate feedback + retry]
    Evidence -->|Yes| Signals[Write .gogo-gadget-satisfied]
    Feedback --> Iteration
```

## 3) Worker Mode + Creative Overseer

Worker mode runs a central execution loop (the "brain") while the Creative Overseer
observes the loop state and injects new capabilities into the executor. The overseer
does not replace the loop; it continuously augments it.

```mermaid
flowchart TB
    subgraph Loop[Worker Main Loop]
        Brain[Coordinator]
        Decompose[TaskDecomposer]
        Execute[WorkerExecutor]
        Aggregate[Aggregate results]
        Verify[Verifier]
    end

    subgraph Agents[Parallel Agents]
        A1[Agent 1]
        A2[Agent 2]
        A3[Agent N]
    end

    subgraph Overseer[Creative Overseer]
        Observe[Observe loop state]
        Brainstorm[Generate capability ideas]
        Synthesize[Create skills/MCPs/agents]
        Register[CapabilityRegistry]
    end

    Brain --> Decompose --> Execute --> Agents --> Aggregate --> Verify --> Brain

    Brain --> Observe
    Observe --> Brainstorm --> Synthesize --> Register
    Register --> Execute
```

## 4) Self-Extend Pipeline (Reactive)

Gap detection monitors output and failure history to create new tools when missing
capabilities block progress.

```mermaid
flowchart LR
    Output[Model output + errors] --> GapDetect[GapDetector]
    GapDetect --> Gap{Gap found?}
    Gap -->|No| Continue[Continue task]
    Gap -->|Yes| Synthesize[SynthesisEngine]
    Synthesize --> Verify[Capability verification]
    Verify --> Registry[CapabilityRegistry]
    Registry --> HotLoad[HotLoader]
    HotLoad --> Continue
```

## 5) RLM (Recursive Language Model) Pipeline

RLM treats large contexts as explorable terrain by chunking, selecting, and
recursively drilling down before synthesizing a final answer.

```mermaid
flowchart TB
    Start[Input query + context path]
    Start --> Chunk[Chunker]
    Chunk --> Navigate[Navigator selects chunks]
    Navigate --> Explore[Explorer reads selected chunks]
    Explore --> Aggregate[Aggregator synthesizes]
    Aggregate --> Answer[Answer + evidence]
```
