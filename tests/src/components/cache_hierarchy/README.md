## Cache Hierarchy

This component implement the functional warming model of the cache hierarchy, including the following modules:
- Private caches, both unified or harvard.
- Non-inclusive shared cache, which can be configured to be exclusive.
- Directory, with MSI coherence protocol, which can be further reconstructed to provide states for MESI protocols.

All caches uses LRU replacement policy.

