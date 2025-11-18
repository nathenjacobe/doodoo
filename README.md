# doodoo: a todo cli written in rust with ratatui

### controls (also shows at the bottom of the cli):
new: [n] | rename: [r] | complete: [↵] | delete: [d] | nav: [↑↓→←],[hjkl] | new/rename page:[1-9] | quit: [q] 

rename to empty string to delete todo / page

hold shift with a navigation key to move a todo / page around

create a todo.json in your current working directory to use that instead of the global one; to stop using the one in your current working directory, move out of your current working directory.
