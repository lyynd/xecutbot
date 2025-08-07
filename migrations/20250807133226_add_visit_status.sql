-- Add status column to visits table
ALTER TABLE visit ADD COLUMN status INTEGER NOT NULL DEFAULT 0;
-- 0: Planned, 1: CheckedIn, 2: CheckedOut
