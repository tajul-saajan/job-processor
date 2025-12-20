-- Rollback: Drop jobs table, index, trigger and function
-- This reverses migration: 20231220000001_create_jobs_table

-- Drop the trigger first
DROP TRIGGER IF EXISTS update_jobs_updated_at ON jobs;

-- Drop the function
DROP FUNCTION IF EXISTS update_updated_at_column();

-- Drop the index
DROP INDEX IF EXISTS idx_jobs_status;

-- Drop the jobs table
DROP TABLE IF EXISTS jobs;
