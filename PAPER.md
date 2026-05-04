# SHERLOCK

## 1. Preface

As part of a one-month proof-of-concept and MVP phase under the role of a
temporary AI Research Fellow, the focus is on validating the core
detection foundations of the system. This includes implementing and
evaluating the Detection Model, with the goal of demonstrating early
effectiveness in identifying fraud patterns and translating model
outputs into clear, actionable insights. This phase serves as a
practical validation step for feasibility, performance, and integration
potential.

## 2. Heterogeneous Data: Synthetic and Historical

All data handling is assumed to operate within a controlled environment
where raw transactional data access is mediated, audited, and compliant
with applicable privacy and security requirements under strict data
governance and access supervision constraints defined by Khalti.

Assuming no initial labeled data, the system plans to incorporate a
synthetic transaction ledger, a statistical transaction synthesizer
modeled using the properties described below, analyzed with the help of
scripts derived from historical Khalti transaction data.

## 2.1. Distribution Metrics
-   Mean, variance, skewness, kurtosis
-   Heavy-tail index (Pareto / power-law behavior)
-   Percentile bands ($10^{\text{th}} \to 90^{\text{th}}$ in increments
    of 10)
-   Marginal distributions of core numeric features

## 2.2. Temporal Dynamics
-   Hour-of-day patterns
-   Day-of-week seasonality
-   Inter-arrival time distribution per user
-   Session clustering (burst vs. periodic behavior)

## 2.3. Relational Structure (Graphs)
-   User-merchant degree distribution
-   Repeat transaction frequency per edge
-   Transaction count per user-merchant pair (edge weight distribution)
-   Unique devices per user distribution
-   Feature covariance structure across numeric variables

The synthesizer generates transactional data that mirrors the
statistical distribution of legitimate traffic while injecting
controlled anomalies.

Following the design of the baseline model, a permutational mix of
anonymized historical and synthetic data blended in suitable ratios
(e.g., 7:3, 8:2, 9:1, 11:1) is proposed to improve detection
reliability.

# 3. Topological Features and Orthogonal Anomaly Ensemble

This layer of the pipeline aims to extract topological features from a
combination of Tarjan's Strongly Connected Components (SCC) and
Johnson's Algorithm for enumerating elementary circuits, along with
latent features from a dual-path latent representation learning system
comprising: (i) Outlier Detection via Deep Information Theory Framework
(ODDITY) and (ii) Deep Convolutional Auto Encoder (DCAE), to ensure
better precision in the final detection step.

## 3.1. Graph Definition

The transaction system is modeled as a *Cyclic Directed Multigraph*
$\mathcal{G} = (\mathcal{V}, \mathcal{E})$:

-   **Nodes** $(\mathcal{V})$: Heterogeneous entities including Account
    IDs, Device IDs, User IDs, and Merchant Endpoints (MID/TID).

-   **Edges** $(\mathcal{E})$: Each edge $e_{ij} \in \mathcal{E}$
    represents a unique transaction $t$, where $t$ contains attributes:
    `amount` and `created_on`.

## Computational Hierarchy

To balance precision with computational scalability, features are
computed at three distinct levels before being mapped to the
edge-centric vector.

### Global Level $(\mathcal{G})$

PageRank $(\text{PR})$ assigns a centrality score to each node:

$$\text{PR}(v) = \frac{1-d}{N} + d \sum_{u \in \mathcal{B}_v} \frac{\text{PR}(u)}{\text{deg}^+(u)}$$

where $\mathcal{B}_v$ denotes the predecessor nodes pointing to $v$,
$\text{deg}^+(u)$ is the degree of departure of node $u$, $d$ is the
damping constant (typically $d = 0.85$), and $N$ is the total number of
nodes.

### Meso-Level (SCC Decomposition)

Tarjan's Algorithm is applied to partition $\mathcal{G}$ into Strongly
Connected Components $(\mathcal{S})$. For each $\mathcal{S}$, Johnson's
Cycle-Finding Algorithm is used to enumerate simple cycles
$(\mathcal{C})$, identifying closed-loop fund transfers. Johnson's
Algorithm is restricted to a depth of $k$ on sampled subgraphs to
maintain $\Theta(V+E)$ time complexity. The exact value of $k$ is to be
fine-tuned based on permutations in the system's performance on changing
it.

### Local and Temporal Level $(\mathcal{L})$

Calculated only for nodes within an SCC; otherwise defaulting to
$\varepsilon$:

-   **K-Core** $(k_v)$: Identifies the maximal subgraph where each node
    has at least degree $k$.

-   **Local Clustering Coefficient** $(C_v)$: Density of the
    neighborhood.

-   **Graph Entropy** $(H_v)$: Diversity of the neighbor degree
    distribution.

-   **Burstiness** $(\mathcal{B}_{ij})$: Temporal intensity of the edge
    $e_{ij}$.

-   **Neighborhood Overlap** $(\mathcal{O}_{ij})$: Jaccard Similarity of
    the sets $\mathcal{N}(i)$ and $\mathcal{N}(j)$.

## Topological Feature Vector

The assembled topological feature vector $\vec{\phi}(e_{ij})$ is
structured as follows:

::: {#tab:topovec}
  **Category**       **Components**              **Description**
  ------------------ --------------------------- -----------------------------------------------------------------------
  Intrinsic          $a,\, t$                    Raw transaction amount and timestamp
  Source Flow        $\text{PR}(i)$              "Exit" capacity of the sender
  Destination Flow   $\text{PR}(j)$              "Entry" capacity of the receiver
  Structural         $k_i, k_j, C_i, C_j$        K-core and Clustering of both endpoints ($\varepsilon$ if not in SCC)
  Relational         $\mathcal{O}_{ij},\, H_v$   Shared neighbor overlap and entropy
  Temporal           $\mathcal{B}_{ij}$          Inter-event burstiness for the $i \to j$ path

  : Topological Feature Vector Components
:::

## Global Graph Medians for Imputation of $\varepsilon$ and Addressing Scalability Issues

Previously mentioned filler values $\varepsilon$ are to be imputed using
global graph medians to ensure the DCAE will learn to distinguish
effectively.

While edge-centric vectors provide granular precision which is ideal for
DCAE, they increase the input dimensionality for downstream stages. To
mitigate scaling issues during Khalti deployment, SHERLOCK proposes an
architectural change perhaps using smart sketch approximations and cache
engineering.

## Orthogonal Anomaly Ensemble

While the Topological Feature Vector maps the structural and temporal
environment of a transaction, the Orthogonal Anomaly Ensemble is
designed to capture deep behavioral deviations and entity-specific risk
profiles.

### Spatial Latent Representation (DCAE Autoencoder)

The Convolutional Deep Dense (DCAE) Autoencoder acts as the primary,
high-throughput pathway. It is specifically designed to ingest the
structured Topological Feature Vector $\vec{\phi}$ and extract spatial
patterns and local feature interactions (e.g., recognizing how a sudden
spike in temporal velocity interacts abnormally with a low Graph
Entropy).

-   **Spatial Reconstruction Error** $(\mathcal{R}_s)$: The Mean Squared
    Error (MSE) between the input topological vector and the DCAE
    Decoder's reconstructed output. High $\mathcal{R}_s$ values indicate
    that the transaction's structural features do not co-vary in
    historically normal ways (change in normal ways).

-   **Spatial Centroid Distance** $(\mathcal{D}_s)$: The Euclidean
    distance of the transaction's bottleneck representation from the
    established "Normal Center" in the latent space. Higher
    $(\mathcal{D}_s)$ suggests deviation from normal.

### Adversarial Latent Representation (ODDITY Framework)

The Outlier Detection via Deep Information Theory Framework (ODDITY)
operates as the secondary, high-fidelity pathway. Because it carries a
higher computational overhead, it is optimized strictly for resisting
adversarial mimicry: scenarios where sophisticated fraudsters
intentionally manipulate transaction amounts, timings, or routing paths
to artificially force their behavior into standard statistical
distributions.

-   **Information-Theoretic Loss** $(\mathcal{L}_{IB})$: By utilizing
    Information Bottleneck constraints, ODDITY learns the true, minimal
    sufficient representation of legitimate behavior. If an adversarial
    transaction attempts to "blend in", it incurs a massive penalty in
    this loss function.

-   **Bottleneck Entropy** $(\mathcal{H}_z)$: Measures the instability
    or "surprise" of the compression for a specific input. Fabricated
    feature overlaps designed to trick standard models will trigger high
    entropy here, as ODDITY struggles to map the synthetic behavior to
    true legitimate manifolds.

### High-Cardinality Metadata Encoding (CatBoost)

To capture the inherent risk associated with specific physical and
digital identities that cannot be modeled as graph properties,
high-cardinality categorical variables are processed using Ordered
Target Encoding.

-   **Merchant Risk Profile** $(\hat{m})$: The encoded representation of
    the `merchant_id`, capturing the historical susceptibility of
    specific merchants or endpoints to laundering loops.

-   **Device Fingerprint Linkage** $(\hat{d})$: A continuous feature
    mapping the risk of the physical hardware, effectively penalizing
    device IDs that exhibit high-velocity account switching.

# Ensemble Detector Pool for Pseudo-Labeling

This stage converts the raw, unlabeled combined feature vectors into
training-ready pseudo-labels. The process runs three independent
unsupervised detectors in parallel over the merged Topological Feature
Vector $\vec{\phi}$ and Orthogonal Anomaly Vector $\vec{\psi}$, then
filters the output before passing it downstream.

## Input

Each transaction is represented by the concatenated vector:
$$\vec{x} = [\vec{\phi},\; \vec{\psi}]$$ where $\vec{\phi}$ contains the
topological features and
$\vec{\psi} = [\mathcal{R}_s,\; \mathcal{H}_z,\; \mathcal{L}_{IB},\; \hat{m},\; \hat{d}]$
contains the orthogonal anomaly features.

## The Three Detectors

Instead of running in parallel, the three detectors operate in a Primary
$\rightarrow$ Secondary $\rightarrow$ Tertiary sequence. This tiered
approach ensures that the majority of transactions are processed with
minimal latency, while \"edge cases\" receive deeper statistical
scrutiny.

1.  **Primary: Isolation Forest.** As the first line of defense, IForest
    recursively partitions the feature space using random axis-aligned
    cuts. Transactions requiring fewer cuts to isolate receive a higher
    anomaly score, making it highly efficient at identifying sparse
    outliers in the high-dimensional topological vector. Output:
    Per-transaction anomaly score $s_{\text{IF}} \in [0, 1]$

2.  **Secondary: Empirical Cumulative Distribution.** Triggered only if
    the IForest score falls within a predefined uncertainty band (e.g.,
    $0.4 < s_{\text{IF}} < 0.6$). It estimates the tail probability of
    each feature dimension independently using the empirical CDF, then
    aggregates them to identify transactions that fall into multiple
    low-probability tails simultaneously. Output: Per-transaction
    anomaly score $s_{\text{ECOD}} \in [0, 1]$.

3.  **Tertiary: Copula-Based Outlier Detection.** Invoked if there is a
    significant divergence between the isolation-based logic of IForest
    and the distribution-based logic of ECOD, defined by a threshold
    $\delta$. COPOD models the joint dependency structure across feature
    dimensions using a copula to capture sophisticated fraud cases where
    individual feature values appear plausible, but their co-occurrence
    is statistically improbable.
    $$|s_{\text{IF}} - s_{\text{ECOD}}| > \delta$$ Output:
    Per-transaction anomaly score $s_{\text{COPOD}} \in [0, 1]$.

## Consensus Anomaly Score

The final consensus score $\hat{s}$ is calculated using a conditional
bypass to optimize for compute cycles while ensuring high-fidelity
resolution for complex cases.

$$\hat{s} = \begin{cases}s_{\text{IF}} & \text{if } |s_{\text{IF}} - 0.5| > \gamma \\ \frac{s_{\text{IF}} + s_{\text{ECOD}} + s_{\text{COPOD}}}{3} & \text{if } (|s_{\text{IF}} - 0.5| \leq \gamma) \lor (|s_{\text{IF}} - s_{\text{ECOD}}| > \delta) \\frac{s_{\text{IF}} + s_{\text{ECOD}}}{2} & \text{otherwise}\end{cases}$$

Using a simple mean rather than a weighted combination avoids
introducing tuning parameters at this pre-label stage.

## Bimodal Filtering Strategy

To minimize label noise, the training set is restricted to the most
unambiguous transactions. The filtering procedure applied to $\hat{s}$
across all $n$ transactions is as follows:

1.  **Pseudo-Fraud label (1).** Assign label $y = 1$ to any transaction
    where $\hat{s} \geq P_{99}(\hat{s})$. These are the top 1% most
    anomalous transactions by consensus.

2.  **Pseudo-Normal label (0).** Assign label $y = 0$ to any transaction
    where $\hat{s} \leq P_{50}(\hat{s})$. These are the bottom 50% of
    transactions by consensus score.

3.  **Exclusion zone.** Discard all transactions where
    $P_{50}(\hat{s}) < \hat{s} < P_{99}(\hat{s})$. This middle band is
    ambiguous and is withheld from the initial training set to prevent
    the supervised model from learning on noisy boundaries.

## Human-in-the-Loop and Audit Queue

To resolve the ambiguity of the \"middle band\" and generate true
ground-truth labels for long-term model calibration, SHERLOCK implements
a semi-supervised feedback loop.

1.  **The Audit Band**: Transactions where
    $P_{90}(\mathcal{S}) < \mathcal{S} < P_{99}(\mathcal{S})$ are
    designated as \"Ambiguous\". Rather than being discarded, these are
    routed to an asynchronous HITL Audit Queue for manual review by
    Khalti's risk operations team.

2.  **Active Learning**: Verified labels from the Audit Queue are
    reintegrated into the training set $\mathcal{D}_{train}$. This
    transforms the Parallel Inference Stack from a purely unsupervised
    learner into a semi-supervised system that adapts to actual
    confirmed fraud patterns rather than just statistical outliers

The resulting labeled dataset
$\mathcal{D}_{\text{train}} = \{(\vec{x}_i, y_i)\}$ is passed directly
to the Parallel Inference Stack.

# Parallel Inference Stack

This stage trains three independent supervised learners on
$\mathcal{D}_{\text{train}}$ from the Ensemble Detector Pool. Each
learner specializes in a different signal type, reducing the risk that
any single fraud pattern bypasses detection.

## Learner Specifications

1.  **Meta-Learner: CatBoost** Consumes the high-cardinality encoded
    identity features $\hat{m}$, $\hat{d}$ produced by Ordered Target
    Encoding. Outputs a fraud probability score
    $p_{\text{CB}} \in [0,1]$ per transaction. CatBoost's native
    handling of categorical variables via ordered target statistics
    makes it the correct choice for this signal type. Its primary role
    is detecting recurring fraudulent actors: merchants and devices with
    historically elevated risk.

2.  **Base-Learner 1: LightGBM** Consumes the full $\vec{\phi}$
    topological vector, with primary weight placed on $\mathcal{R}_s$
    (Spatial Reconstruction Error). Outputs fraud probability
    $p_{\text{LGBM}} \in [0,1]$. Configured with asymmetric cost
    weighting to penalize false negatives more than false positives,
    ensuring broad coverage of structurally anomalous transactions. This
    learner targets volume fraud and "sloppy" patterns that deviate
    clearly from historical graph norms.

3.  **Base-Learner 2: PKBoost** PKBoost is an adaptive gradient boosting
    technique library written in Rust that has proven to be "drift
    resilient" especially in anomaly detection. Prioritizes adversarial
    latent features $\mathcal{L}_{IB}$ and $\mathcal{H}_z$ from ODDITY,
    with Shannon Information Gain used as the primary split criterion.
    Outputs fraud probability $p_{\text{PK}} \in [0,1]$. Targets
    sophisticated mimicry: transactions that have been deliberately
    calibrated to appear normal in standard statistical distributions
    but carry high information-theoretic cost under the bottleneck
    constraint.

# Decision Fusion

This stage aggregates the three probability outputs from the Parallel
Inference Stack into a single calibrated "Final Verdict" score using a
Logistic Regression Meta-Classifier. The logistic layer learns the
optimal weighting across the three signals on a held-out validation
split, preventing any single learner from dominating the final output
regardless of its individual precision on training data.

## Meta-Classifier Formulation

Let $\vec{p} = [p_{\text{CB}},\, p_{\text{LGBM}},\, p_{\text{PK}}]$ be
the probability vector from the Parallel Inference Stack. The
meta-classifier computes the final fraud probability as:

$$P_{\text{fraud}} = \sigma\!\left(w_0 + w_1 p_{\text{CB}} + w_2 p_{\text{LGBM}} + w_3 p_{\text{PK}}\right)$$

where $\sigma(\cdot)$ is the sigmoid function and weights
$w_0, w_1, w_2, w_3$ are learned on a held-out validation split of
$\mathcal{D}_{\text{train}}$.

A binary verdict is produced by applying a decision threshold $\tau$:

$$\hat{y} = \begin{cases} 1 & \text{if } P_{\text{fraud}} \geq \tau \\ 0 & \text{otherwise} \end{cases}$$

The threshold $\tau$ is tuned on the validation set to optimize the
$F_\beta$ score, with $\beta > 1$ to weight recall over precision,
reflecting the higher cost of a missed fraud versus a false alert.

## Signal Roles in Fusion

The weighted combination explicitly accounts for the three distinct
signal types. A transaction must register anomalously across multiple
signal types to reach a high $P_{\text{fraud}}$, which reduces false
positives from any single noisy detector.
