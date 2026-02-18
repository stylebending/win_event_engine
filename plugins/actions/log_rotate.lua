-- Log rotation example
-- Rotates log files when they exceed 10MB

function on_event(event)
    local log_path = event.metadata.path
    
    if not log_path then
        return {success = true, message = "No path"}
    end
    
    -- Only process .log files
    if not string.find(log_path, "%.log$") then
        return {success = true, message = "Not a log file"}
    end
    
    -- Check file size (10MB = 10 * 1024 * 1024 bytes)
    local max_size = 10 * 1024 * 1024
    local size = fs.file_size(log_path)
    
    if size < max_size then
        log.debug("Log file size OK: " .. log_path .. " (" .. size .. " bytes)")
        return {success = true, message = "File size OK"}
    end
    
    log.info("Log file exceeds 10MB, rotating: " .. log_path)
    
    -- Create rotated filename with timestamp
    local timestamp = os.date("%Y%m%d_%H%M%S")
    local rotated_path = log_path .. "." .. timestamp
    
    -- Move current log to rotated name
    if not fs.move(log_path, rotated_path) then
        log.error("Failed to rotate log: " .. log_path)
        return {success = false, message = "Failed to rotate log"}
    end
    
    log.info("Log rotated: " .. rotated_path)
    
    -- Compress the rotated log using PowerShell
    local zip_path = rotated_path .. ".zip"
    local compress_result = exec.run("powershell.exe", {
        "-Command",
        "Compress-Archive -Path '" .. rotated_path .. "' -DestinationPath '" .. zip_path .. "' -Force"
    })
    
    if compress_result.exit_code == 0 then
        log.info("Log compressed: " .. zip_path)
        
        -- Delete the uncompressed rotated file
        if fs.delete(rotated_path) then
            log.info("Uncompressed rotated log deleted: " .. rotated_path)
        end
    else
        log.warn("Failed to compress log: " .. compress_result.stderr)
    end
    
    return {success = true, message = "Log rotated successfully"}
end
