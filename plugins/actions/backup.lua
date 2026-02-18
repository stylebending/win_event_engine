-- Smart backup example
-- Only backs up files larger than 1MB

function on_event(event)
    local path = event.metadata.path
    
    if not path then
        log.warn("No path in event metadata")
        return {success = true, message = "No path to backup"}
    end
    
    -- Check file size
    local size = fs.file_size(path)
    
    if size < 0 then
        log.error("Cannot get file size: " .. path)
        return {success = false, message = "Cannot get file size"}
    end
    
    local size_mb = size / (1024 * 1024)
    
    if size_mb < 1 then
        log.info("File too small for backup: " .. path .. " (" .. string.format("%.2f", size_mb) .. " MB)")
        return {success = true, message = "File too small, skipped"}
    end
    
    log.info("Large file detected: " .. path .. " (" .. string.format("%.2f", size_mb) .. " MB)")
    
    -- Create backup path with timestamp
    local filename = fs.basename(path)
    local timestamp = os.date("%Y%m%d_%H%M%S")
    local backup_path = "backups/" .. timestamp .. "_" .. filename
    
    -- Copy file using system command (more reliable for large files)
    local result = exec.run("cmd.exe", {"/c", "copy", path, backup_path})
    
    if result.exit_code == 0 then
        log.info("Backup successful: " .. backup_path)
        return {success = true, message = "Backed up to " .. backup_path}
    else
        log.error("Backup failed: " .. result.stderr)
        return {success = false, message = "Backup failed: " .. result.stderr}
    end
end
