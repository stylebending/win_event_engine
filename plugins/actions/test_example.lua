-- Simple test script for the Lua plugin system
-- This demonstrates all the available APIs

function on_event(event)
    -- Log the event details
    log.info("========================================")
    log.info("Event received!")
    log.info("Type: " .. event.kind)
    log.info("Source: " .. event.source)
    log.info("Time: " .. event.timestamp)
    
    -- Log metadata if available
    if event.metadata then
        log.info("Metadata:")
        for key, value in pairs(event.metadata) do
            log.info("  " .. key .. " = " .. value)
        end
    end
    
    -- Test JSON encoding
    local test_data = {
        event_type = event.kind,
        source = event.source,
        processed = true
    }
    local json_str = json.encode(test_data)
    log.info("JSON encoded: " .. json_str)
    
    -- Test JSON decoding
    local decoded = json.decode(json_str)
    log.info("JSON decoded event_type: " .. decoded.event_type)
    
    -- Get current time
    local current_time = os.date("%Y-%m-%d %H:%M:%S")
    log.info("Current time: " .. current_time)
    
    -- Example: Check if a file exists (if path is in metadata)
    if event.metadata and event.metadata.path then
        local path = event.metadata.path
        log.info("Checking file: " .. path)
        
        if fs.exists(path) then
            local size = fs.file_size(path)
            log.info("File exists! Size: " .. size .. " bytes")
            
            local filename = fs.basename(path)
            log.info("Filename: " .. filename)
        else
            log.warn("File not found: " .. path)
        end
    end
    
    log.info("========================================")
    
    -- Always return success for this test
    return {
        success = true,
        message = "Test script executed successfully!"
    }
end
