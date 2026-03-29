# Why Not `tmux`

I had several reasons for that.

I used `tmux` for a while, not for very long, but long enough to understand its overall model, how it is installed, and how it is usually configured. Some of my impressions may already be outdated, but I understood one thing fairly quickly: `tmux` is solving a different problem from the one I actually had.

## 1. It is a different tool with a different center of gravity

`tmux` is very strong as a session multiplexer:

- detach / attach;
- surviving broken connections;
- long-lived processes;
- remote work.

I do not need that.

My workflow is strictly local. I am not trying to keep processes alive after closing the terminal, and I am not building a tool for unstable connections. I want a local workspace with my own behavioral model and my own keyboard shortcuts.

So the problem was never that `tmux` is bad. The problem was that it is optimized for a different use case.

## 2. I do not want to live in constant configuration mode

I do not enjoy spending time on configs for a personal tool.

If comfortable work requires me to:

- install a base tool;
- remember how it is configured;
- pick a set of options;
- add exceptions on top;
- and later remember why everything is set up that way,

then for me that is already a bad sign.

I want the tool to reflect the workflow I need from the start, rather than forcing me to adapt myself to someone else's model or assemble my own environment on top of it from pieces.

## 3. I do not like plugins for personal needs

That is a separate reason.

Yes, many things in `tmux` can be added with plugins, including parts of persistence. But that is exactly the path I do not like.

### 3.1. Extra cognitive load

If I only had to remember plain `tmux`, that would still be tolerable.

But with plugins, I have to remember not only the tool itself, but also:

- which plugins are installed;
- which one is responsible for what;
- how they are configured;
- where the boundary is between `tmux` itself and a particular plugin.

I do not like that mental model.

### 3.2. Plugins almost always increase fragility

For a personal environment, plugins very often mean:

- more failure points;
- more conflicts;
- weaker maintenance;
- and a less transparent system overall.

For a personal tool, I want the opposite:

- fewer layers;
- fewer dependencies;
- fewer places where things can unexpectedly drift apart.

## 4. “For myself” is already a sufficient reason

For me, this is probably the main argument.

If I need a tool that fits the way I work, then the desire to make it fit me is already a strong enough reason on its own. I am not required to use `tmux` just because it already exists and does many things well.

Especially now, the cost of creating such a tool has dropped a lot. If in the past the idea of “writing your own terminal manager for yourself” sounded like an excessive luxury, now it is a practical path.

I started building this tool at 8 in the morning, and by 15:00 I already had the first working version and started using it.

For me, that is a very strong argument in favor of a custom solution.

Not in the sense of “I replaced all of `tmux` in one day.” Of course not.

But in the sense that:

- I got the behavior I needed very quickly;
- I did not have to force myself into someone else's model;
- the tool immediately became useful in my real workflow.

## In Short

It was not that `tmux` was literally “missing” something for me.

It was more like this:

- `tmux` is solving a slightly different problem;
- I do not want to assemble my personal environment out of a base tool plus plugins;
- my own keys, my own logic, and a local workflow matter to me;
- if I can quickly build a tool for myself and start using it immediately, that is already more than enough reason to do exactly that.
