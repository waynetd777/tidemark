## Ask

**Ask** (⌘K) is a chat with an AI about your computer, using what Tidemark knows.

- Ask why the computer was slow on Wednesday, or where space can be freed safely. Start from a suggested question or type your own; follow-ups continue the conversation.
- It is given the folder on screen and its biggest folders, what grew this week, and the performance record: CPU, GPU, memory and its split, network, energy, today's highs, the week's tidemarks, incidents, crashes, and the apps that used the most. It is also given this guide, so it reads the figures as Tidemark means them and can answer questions about the app.
- On Now, an **Ask** button sits beside each incident in This week's tidemarks, each crash in Crashes and hangs this week, and any card flagged amber or red. It opens Ask with a question and that thing's details attached: an incident's apps at its peak, a summary of a crash's latest reports (<!-- mac -->the exception, how it ended and the crashed thread's stack<!-- /mac --><!-- github --> on a Mac; on Windows, <!-- /github --><!-- windows -->the app, its version, the faulting module and the exception code<!-- /windows -->), a chart's figures and what each app is using now. **Remove** drops them, leaving the question.
- It runs through an AI command-line tool already signed in on this computer: Claude Code, Codex, Antigravity or GitHub Copilot. No key is set up in the app; with none installed, Ask is disabled and says why.
- The model menu lists the models those tools offer (Copilot offers only Auto). Claude Opus is the default; your choice is remembered.
- Whatever you ask goes to that tool's provider (Anthropic, OpenAI, Google or GitHub), under your own account. The AI is given names, sizes, paths and figures, and a crash's report summary when you ask about one. Each tool runs in an empty folder of its own with its tools switched off, so it can't read your files, run commands or search the web. Tidemark changes nothing: it advises, you act.
- **Copy** copies an answer as text and formatted text. **New chat** starts over.

<!-- windows -->
On Windows, install the tool in the Windows account you use Tidemark in, then sign in by running it once in a terminal (PowerShell):

| Tool | Install it with |
|---|---|
| Claude Code | `irm https://claude.ai/install.ps1 \| iex` in PowerShell |
| Codex | `npm install -g @openai/codex` |
| GitHub Copilot | `npm install -g @github/copilot` |
| Antigravity | Its own installer |

A tool installed after Tidemark started may not be found until Tidemark is restarted, or you sign out and in again.
<!-- /windows -->
