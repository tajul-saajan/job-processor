-- Rollback: Drop jobs table and index
-- This reverses migration: 20231220000001_create_jobs_table

-- Drop the index first
DROP INDEX IF EXISTS idx_jobs_status;

-- Drop the jobs table
DROP TABLE IF EXISTS jobs;
