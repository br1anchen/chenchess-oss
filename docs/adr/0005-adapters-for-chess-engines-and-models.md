# Adapters for chess engines and models

ChenChess will place adapter interfaces in front of external chess engines and models, including Stockfish for Engine Analysis and Maia for the Human Move Model. The MVP uses local/self-hosted Stockfish and Maia, but adapters preserve the option for self-hosters to swap different objective engines or human-move models without changing the Game Review pipeline.
