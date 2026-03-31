-- Add extraction metadata columns to reports
ALTER TABLE reports ADD COLUMN model_used TEXT;
ALTER TABLE reports ADD COLUMN agent_turns INTEGER DEFAULT 0;
ALTER TABLE reports ADD COLUMN extracted_count INTEGER DEFAULT 0;
ALTER TABLE reports ADD COLUMN unresolved_count INTEGER DEFAULT 0;
