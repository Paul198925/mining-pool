-- KawPoW (Ravencoin). Its miners are told a target, so the difficulty is in plain
-- Bitcoin units: a GPU doing 20 MH/s finds a share of difficulty 0.07 every 15 seconds.
INSERT INTO algos (name, port, start_difficulty, min_difficulty, max_difficulty) VALUES
    ('kawpow', 3336, 0.05, 0.0001, 100000);
