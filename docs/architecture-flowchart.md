# Jarvis-V2 System Architecture

## High-Level Overview

```mermaid
flowchart TB
    subgraph User["👤 User"]
        Task[/"Task: Add JWT auth and PostgreSQL"/]
    end

    subgraph Jarvis["🤖 Jarvis-V2"]
        subgraph Analysis["Task Analysis"]
            TA[Task Analyzer]
            TA --> |Complexity Score| Decision{Mode?}
        end

        Decision --> |Simple| Single[Single Agent]
        Decision --> |Complex| Swarm[Swarm Mode]

        subgraph SwarmSystem["🐝 Swarm System"]
            subgraph Brain["Brain (SwarmCoordinator)"]
                Decompose[Task Decomposer]
                Execute[Swarm Executor]
                Verify[Verifier]
                Aggregate[Result Aggregator]
            end

            subgraph Overseer["🎨 Creative Overseer"]
                Observe[Observe Context]
                Brainstorm[Brainstorm Ideas]
                Synthesize[Synthesize Capabilities]
            end

            subgraph SharedState["📋 SharedContext"]
                TaskInfo[Task + Domains + Tech]
                Phase[Current Phase]
                Subtasks[Subtasks]
                Suggestions[Capability Suggestions]
            end

            subgraph Agents["🤖 Parallel Agents"]
                A1[Agent 1: Implementation]
                A2[Agent 2: Review]
                A3[Agent N: ...]
            end
        end

        subgraph Capabilities["🧰 Capability System"]
            Registry[(Capability Registry)]
            Skills[Skills/*.md]
            MCPs[MCP Servers]
            AgentDefs[Agents/*.md]
            HotLoad[Hot Loader]
        end
    end

    Task --> TA
    Swarm --> Decompose
    Decompose --> |Updates| SharedState
    SharedState --> |Observes| Observe
    Decompose --> Execute
    Execute --> Agents
    Agents --> |Results| Aggregate
    Aggregate --> Verify
    Verify --> |Next Iteration| Decompose

    Observe --> Brainstorm
    Brainstorm --> |High Confidence| Synthesize
    Synthesize --> Skills
    Synthesize --> MCPs
    Synthesize --> AgentDefs
    Skills --> HotLoad
    MCPs --> HotLoad
    AgentDefs --> HotLoad
    HotLoad --> Registry
    Registry --> |Available to| Agents

    Suggestions --> |"Suggests to Brain"| Brain

    style Overseer fill:#e1f5fe
    style Brain fill:#fff3e0
    style SharedState fill:#f3e5f5
    style Capabilities fill:#e8f5e9
```

## Detailed Execution Flow

```mermaid
sequenceDiagram
    autonumber
    participant U as 👤 User
    participant J as 🤖 Jarvis CLI
    participant B as 🧠 Brain (Coordinator)
    participant C as 📋 SharedContext
    participant O as 🎨 Creative Overseer
    participant E as ⚡ Executor
    participant A as 🤖 Agents
    participant S as 🔧 Synthesis Engine
    participant R as 📦 Registry

    U->>J: jarvis-v2 --swarm 2 "Add JWT auth..."
    J->>B: execute_with_overseer(task)

    Note over B,C: Phase: Analyzing
    B->>C: start_task(task)
    C->>C: Extract domains, technologies

    par Brain executes task
        Note over B,E: Phase: Decomposing
        B->>C: set_phase(Decomposing)
        B->>E: decompose(task)
        E-->>B: subtasks[]
        B->>C: set_subtasks(subtasks)

        Note over B,A: Phase: Executing
        B->>C: set_phase(Executing)
        B->>E: execute(subtasks)
        E->>A: spawn parallel agents
        A->>A: Work on subtasks
        A-->>E: agent_results[]

        Note over B: Phase: Aggregating
        B->>C: set_phase(Aggregating)
        E-->>B: SwarmResult

        Note over B: Phase: Verifying
        B->>C: set_phase(Verifying)
        B->>B: verify_result()

    and Overseer observes & creates
        loop Every 10 seconds
            O->>C: snapshot()
            C-->>O: TaskSnapshot
            O->>O: Analyze domains/tech
            O->>S: synthesize(ideas)
            S-->>O: capabilities
            O->>R: register(capability)
            O->>C: suggest_capability(name)
        end
    end

    B-->>J: SwarmResult
    J-->>U: Task Complete
```

## SharedContext State Machine

```mermaid
stateDiagram-v2
    [*] --> Analyzing: start_task()

    Analyzing --> Decomposing: Task analyzed
    Decomposing --> Executing: Subtasks created
    Executing --> Aggregating: Agents complete
    Aggregating --> Verifying: Results merged

    Verifying --> Complete: Verification passed
    Verifying --> Decomposing: Next iteration
    Verifying --> Failed: Blocked

    Complete --> [*]
    Failed --> [*]

    note right of Analyzing
        Overseer starts observing
        Domains/tech detected
    end note

    note right of Executing
        Overseer creates capabilities
        proactively based on context
    end note
```

## Capability Synthesis Pipeline

```mermaid
flowchart LR
    subgraph Detection["🔍 Detection"]
        Context[Task Context]
        Domains[Detected Domains]
        Tech[Technologies]
    end

    subgraph Ideation["💡 Ideation"]
        Prompt[Build Observation Prompt]
        Claude[Claude API]
        Ideas[Capability Ideas]
    end

    subgraph Synthesis["🔨 Synthesis"]
        Template[Load Template]
        Generate[Generate Content]
        Verify[Verify Capability]
    end

    subgraph Registration["📦 Registration"]
        Save[Save to Disk]
        HotLoad[Hot Load to Claude]
        Register[Add to Registry]
    end

    Context --> Prompt
    Domains --> Prompt
    Tech --> Prompt
    Prompt --> Claude
    Claude --> Ideas
    Ideas --> |confidence >= 0.7| Template
    Template --> Generate
    Generate --> Verify
    Verify --> |Success| Save
    Save --> HotLoad
    HotLoad --> Register
    Register --> |Suggest| Brain

    style Detection fill:#e3f2fd
    style Ideation fill:#fff8e1
    style Synthesis fill:#f3e5f5
    style Registration fill:#e8f5e9
```

## Component Responsibilities

```mermaid
mindmap
    root((Jarvis-V2))
        Brain
            Task Decomposition
            Agent Orchestration
            Conflict Resolution
            Verification Loop
            Context Updates
        Creative Overseer
            Context Observation
            Domain Detection
            Capability Brainstorming
            Proactive Synthesis
            Suggestion to Brain
        SharedContext
            Task State
            Phase Tracking
            Domain/Tech Detection
            Capability Suggestions
            Thread-Safe Access
        Capability System
            Synthesis Engine
            Template Registry
            Hot Loader
            Capability Registry
            Verification
        Swarm Executor
            Parallel Agent Spawn
            Result Collection
            Conflict Detection
            Progress Tracking
```

## File Structure

```
src/
├── main.rs                 # CLI entry point
├── swarm/
│   ├── mod.rs              # Swarm types
│   ├── coordinator.rs      # Brain (orchestration)
│   ├── executor.rs         # Parallel agent execution
│   └── decomposer.rs       # Task decomposition
└── extend/
    ├── mod.rs              # Capability types
    ├── context.rs          # SharedContext
    ├── overseer.rs         # Creative Overseer
    ├── synthesis.rs        # Capability synthesis
    ├── loader.rs           # Hot loading
    ├── registry.rs         # Capability registry
    └── verify.rs           # Capability verification
```

## Key Data Flows

| Flow | From | To | Data |
|------|------|-----|------|
| Task Start | Brain | SharedContext | task, domains, technologies |
| Phase Update | Brain | SharedContext | TaskPhase enum |
| Observation | SharedContext | Overseer | TaskSnapshot |
| Synthesis | Overseer | Registry | SynthesizedCapability |
| Suggestion | Overseer | SharedContext | capability name |
| Discovery | Brain | SharedContext | suggested capabilities |

## Usage

```bash
# Run with Creative Overseer (default)
jarvis-v2 --swarm 3 "Add authentication to the API"

# Disable overseer for faster execution
jarvis-v2 --swarm 3 --no-overseer "Quick fix"

# Standalone capability synthesis
jarvis-v2 --overseer-only "Build a user management system"
```
