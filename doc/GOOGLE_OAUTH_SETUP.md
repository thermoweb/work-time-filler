# Google OAuth Setup Guide

## Understanding OAuth Credentials

### What's in `credentials.json`?

The `credentials.json` file contains **app credentials**, not user credentials:
- `client_id` - Identifies your application
- `client_secret` - Secret key for your application
- `redirect_uris` - Where Google sends users after authentication

**Important:** These credentials identify YOUR APP, not individual users.

### What's in `token.json`?

The `token.json` file (created during OAuth flow) contains **user credentials**:
- `access_token` - Short-lived token for API access
- `refresh_token` - Long-lived token to get new access tokens
- User-specific authorization data

**This file is personal and should never be shared.**

## Options for Beta Testing

### Option 1: Share App Credentials (Recommended for Internal Testing)

✅ **Best for:** 2-10 trusted team members  
✅ **Security:** Medium (suitable for internal tools)  
✅ **Setup time:** 5 minutes  

**How it works:**
1. You create ONE OAuth app in Google Cloud Console
2. You add team members as test users
3. You share `credentials.json` with team (via secure channel)
4. Each user runs their own OAuth flow
5. Each user gets their own `token.json` (personal, not shared)

**Pros:**
- ✅ Simple setup for users
- ✅ You control the app (can revoke if needed)
- ✅ All users share your quota (usually 10,000 requests/day - plenty for small team)
- ✅ Each user's data stays private (separate tokens)

**Cons:**
- ⚠️ `credentials.json` should not be committed to public GitHub
- ⚠️ Users see "unverified app" warning (acceptable for internal use)
- ⚠️ Limited to 100 test users

**How to set up:**

1. **Create OAuth App** (Google Cloud Console)
   - Go to https://console.cloud.google.com/
   - Create project: "WTF Internal"
   - Enable Google Calendar API
   - Create OAuth 2.0 credentials (Desktop app)
   - Download `credentials.json`

2. **Configure OAuth Consent Screen**
   - User type: **Internal** (if G Workspace) or **External** (Testing mode)
   - Add test users: your team members' emails
   - Scopes: `https://www.googleapis.com/auth/calendar.readonly`

3. **Share Credentials with Team**
   ```bash
   # Share via secure channel (Slack DM, encrypted email, etc.)
   # DO NOT commit to public GitHub
   
   # Users place it in their config directory
   # On init, WTF will prompt for this path
   ```

4. **Each User Does OAuth**
   ```bash
   wtf init  # Will trigger OAuth flow
   # Browser opens, user logs in with their Google account
   # User grants permission to YOUR app
   # token.json created (user-specific, stays on their machine)
   ```

### Option 2: Each User Creates Their Own App (Most Secure)

✅ **Best for:** Public distribution, maximum security  
✅ **Security:** Highest  
✅ **Setup time:** 15-20 minutes per user  

**How it works:**
1. Each user creates their own OAuth app in Google Cloud Console
2. Each user downloads their own `credentials.json`
3. Each user uses their own quota

**Pros:**
- ✅ Maximum security (no shared secrets)
- ✅ Each user has their own quota
- ✅ No trust required

**Cons:**
- ❌ Complex setup for non-technical users
- ❌ Each user needs Google Cloud Console access
- ❌ More support burden

**When to use:** Only for public releases or highly sensitive scenarios.

### Option 3: Skip Google Calendar for Beta

✅ **Best for:** Quick beta testing focused on Jira  
✅ **Security:** N/A  
✅ **Setup time:** 0 minutes  

Just tell testers to say "No" when `wtf init` asks about Google Calendar.

## Recommendation for Your Beta

**Use Option 1** (Shared App Credentials):

1. Create ONE OAuth app in your Google Cloud Console
2. Add your 2-3 team members as test users
3. Share `credentials.json` via Slack/email (don't commit to git)
4. Users run `wtf init` and complete OAuth individually
5. Each user gets their own `token.json`

**This is the standard approach for internal tools!**

## Security Best Practices

### ⚠️ CRITICAL: Never Commit Credentials to Public GitHub

**DO NOT commit `credentials.json` to public repositories**, even though it's "just" app credentials.

**Why it's dangerous:**
1. **Quota Abuse** - Anyone can use YOUR client_id to make requests, exhausting your API quota
2. **Client Secret is SECRET** - The `client_secret` should never be public (the name says it all!)
3. **Can't Rotate Easily** - Once public, anyone who downloaded it has a copy forever
4. **Spam/Abuse** - Bad actors could abuse your app identity for spam or malicious purposes

**What happens if leaked:**
- ⚠️ Your Google API quota gets consumed by others
- ⚠️ Google may suspend your app for abuse
- ⚠️ You'll need to regenerate credentials and redistribute to team
- ⚠️ All users need to get new credentials and re-authenticate

### For You (App Owner)

✅ **DO:**
- Keep `credentials.json` private (gitignore it)
- Share only with trusted team members
- Use "Internal" OAuth consent if you have G Workspace
- Monitor API quota usage
- Can revoke client_secret anytime if compromised

❌ **DON'T:**
- Commit `credentials.json` to public GitHub
- Share with untrusted users
- Publish client_secret publicly

### For Users

✅ **DO:**
- Keep `token.json` private (never share)
- Revoke access if device is lost/stolen
- Review permissions during OAuth flow

❌ **DON'T:**
- Share your `token.json` file
- Commit `token.json` to git

## Quota Limits

**Google Calendar API (Free Tier):**
- 1,000,000 queries/day (shared across all users)
- 10 requests/second per user

**For internal team (2-10 users):**
- Plenty of quota for daily syncs
- Won't hit limits with normal usage

**If you hit quota limits:**
- Request quota increase (usually approved)
- Or switch to Option 2 (each user creates own app)

## Handling "Unverified App" Warning

When users do OAuth, they'll see:
> "This app hasn't been verified by Google"

**For internal testing, this is normal!** Users can click "Advanced" → "Go to WTF (unsafe)" to proceed.

**To remove this warning:**
- Submit app for Google verification (takes weeks)
- Or use "Internal" OAuth consent (G Workspace only)
- Or have users create their own apps

For beta testing, just document this in instructions.

## Revoking Access

**If credentials are compromised:**
1. Go to Google Cloud Console
2. OAuth 2.0 Client IDs
3. Delete or regenerate client_secret
4. Download new `credentials.json`
5. Share updated file with team

**If user wants to revoke:**
1. Go to https://myaccount.google.com/permissions
2. Find "WTF" app
3. Click "Remove Access"

## For Future: Publishing to Public

When ready for public release:

1. **Option A: Get Verified**
   - Submit for OAuth verification
   - Provides privacy policy, terms of service
   - Takes 2-6 weeks
   - Users won't see warning

2. **Option B: Instruct Users to Create Own**
   - Provide detailed guide
   - More secure but more complex
   - Each user gets own quota

3. **Option C: Publish to crates.io**
   - Document OAuth setup in README
   - Provide setup script
   - Most common for CLI tools

## Summary

**For your 2-3 team members:**
```
1. You: Create OAuth app in Google Cloud Console
2. You: Add team emails as test users
3. You: Share credentials.json (Slack/email, NOT git)
4. Them: Run `wtf init`, complete OAuth
5. Them: Get their own token.json
6. Done! Each user's calendar data stays private.
```

This is the standard practice for internal tools and totally fine! 🎉
