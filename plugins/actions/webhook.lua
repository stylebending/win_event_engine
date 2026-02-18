-- Webhook notification example
-- Sends event data to a Discord webhook

function on_event(event)
    local webhook_url = "https://discord.com/api/webhooks/YOUR_WEBHOOK_ID/YOUR_WEBHOOK_TOKEN"
    
    -- Build the Discord embed
    local payload = {
        content = "Event detected!",
        embeds = {
            {
                title = event.kind,
                description = "Source: " .. event.source,
                fields = {
                    {name = "Event ID", value = event.id, inline = true},
                    {name = "Time", value = event.timestamp, inline = true}
                },
                color = 3447003  -- Blue
            }
        }
    }
    
    -- Add file path if available
    if event.metadata.path then
        table.insert(payload.embeds[1].fields, {
            name = "Path",
            value = event.metadata.path,
            inline = false
        })
    end
    
    -- Send to Discord
    local result = http.post(webhook_url, {
        body = json.encode(payload),
        headers = {
            ["Content-Type"] = "application/json"
        }
    })
    
    if result.status == 204 or result.status == 200 then
        log.info("Discord notification sent successfully")
        return {success = true}
    else
        log.error("Failed to send Discord notification: HTTP " .. result.status)
        return {success = false, message = "HTTP " .. result.status}
    end
end
