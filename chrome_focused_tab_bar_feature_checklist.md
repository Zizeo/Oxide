# Modern Browser Tab Bar --- Complete Feature Checklist (Chrome-focused)

> A practical product and implementation checklist for a modern browser
> tab bar, using **Google Chrome on desktop** as the primary reference.
>
> **Scope note:** This document distinguishes between features available
> in Chrome, Chrome behaviors that vary by platform/version, and
> optional ideas for a custom Chromium-based browser. Not every feature
> listed is a native Chrome feature.

## Status legend

-   **Chrome** --- available in current or established desktop Chrome
    behavior.
-   **Chrome / version-dependent** --- availability or exact behavior
    can vary by Chrome version, operating system, rollout, or settings.
-   **Custom / optional** --- useful feature idea, but not a standard
    built-in Chrome tab feature.

------------------------------------------------------------------------

## 1. Core tab interactions

  -----------------------------------------------------------------------
  Feature                 What it does            Status
  ----------------------- ----------------------- -----------------------
  Open a new tab          Create a new tab,       Chrome
                          normally with `Ctrl+T`. 

  Close a tab             Close the current tab   Chrome
                          using its close button  
                          or `Ctrl+W`.            

  Activate a tab          Click a tab to display  Chrome
                          its page.               

  Switch tabs             Move between open tabs  Chrome
                          using the mouse or      
                          keyboard.               

  New Tab button          Create a tab using the  Chrome
                          `+` button beside the   
                          tab strip.              

  Tab close button        Show an `×` close       Chrome
                          control on tabs,        
                          subject to available    
                          space and UI behavior.  

  Tab scrolling /         Navigate the tab strip  Chrome /
  overflow                when many tabs are      version-dependent
                          open.                   

  Tab search              Search and select open  Chrome
                          tabs from Chrome's      
                          tab-search interface.   

  Tab hover card          Show a tab preview card Chrome
                          when hovering over a    
                          tab.                    

  Tab title tooltip       Reveal the full title   Chrome
                          when the title is       
                          truncated.              

  Favicon                 Display the website's   Chrome
                          icon.                   

  Loading indicator       Show loading activity   Chrome
                          while a page is         
                          loading.                

  Audio indicator         Indicate that a tab is  Chrome
                          playing audio.          

  Muted indicator         Indicate that audio     Chrome
                          from a tab is muted.    

  Crash / error state     Communicate that a page Chrome
                          has crashed or failed.  

  Tab discard / memory    A tab may be discarded  Chrome
  state                   to release resources    
                          and reloaded when       
                          selected.               

  Custom dirty-state      Show unsaved changes    Custom / app-dependent
  indicator               for web apps when       
                          supported by the app.   
  -----------------------------------------------------------------------

## 2. Dragging, dropping and reordering tabs

  -----------------------------------------------------------------------
  Feature                 What it does            Status
  ----------------------- ----------------------- -----------------------
  Reorder a tab           Drag a tab left or      Chrome
                          right within the tab    
                          strip.                  

  Move a tab into a group Drag a tab onto or into Chrome
                          a tab group.            

  Remove a tab from a     Drag it out of its      Chrome
  group                   group.                  

  Move a tab to another   Drag a tab to another   Chrome
  window                  Chrome window.          

  Detach a tab            Drag a tab away from    Chrome
                          its window to create a  
                          separate window.        

  Drag a group            Move a tab group and    Chrome
                          its tabs together.      

  Drag multiple selected  Move a selection of     Chrome
  tabs                    tabs together.          

  Drag tab to a specific  Insert the tab at the   Chrome
  insertion point         indicated position.     

  Drag auto-scroll        Scroll the tab strip    Chrome /
                          when dragging near its  version-dependent
                          edge.                   

  Drop-position indicator Show where the dragged  Chrome /
                          tab or group will be    version-dependent
                          inserted.               

  Drag a link to the tab  Open a link as a tab,   Chrome
  strip                   depending on the drop   
                          target.                 

  Drag a URL into the tab Open the URL in a tab.  Chrome
  strip                                           

  Drag a tab outside the  Detach it into a new    Chrome
  browser                 window.                 

  Drag a tab onto another Group tabs by dropping  Chrome /
  tab to create a group   one onto another or     version-dependent
                          using the group UI.     

  Drag a tab to the       Bookmark a link or page Chrome
  bookmarks bar           where supported by the  
                          drop target.            

  Custom drag threshold / Configure drag          Custom / optional
  animation               sensitivity, animations 
                          or snapping behavior.   
  -----------------------------------------------------------------------

**Recommended interaction rules for a custom browser**

-   A normal drag moves one tab.
-   If several tabs are selected, dragging one selected tab should move
    the whole selection.
-   Dragging a tab into a group adds it to that group.
-   Dragging a tab outside the window detaches it.
-   Always show a clear insertion marker and group drop target.
-   Avoid accidental detachment by using a small drag threshold.

## 3. Multi-selection

Chrome supports selecting multiple tabs on desktop. This enables bulk
operations such as moving, closing, muting, bookmarking or grouping
selected tabs, subject to the operation and Chrome version.

  -----------------------------------------------------------------------
  Feature                 What it does            Status
  ----------------------- ----------------------- -----------------------
  `Ctrl` + click          Add or remove an        Chrome
                          individual tab from the 
                          selection on            
                          Windows/Linux.          

  `Cmd` + click           Add or remove an        Chrome
                          individual tab from the 
                          selection on macOS.     

  `Shift` + click         Select a continuous     Chrome
                          range of tabs.          

  Select all tabs         Select all tabs in the  Chrome
                          window where supported  
                          by the tab context      
                          menu.                   

  Visual selection state  Highlight selected tabs Chrome
                          separately from the     
                          active tab.             

  Active vs selected      The active tab is the   Chrome
  distinction             displayed page;         
                          selected tabs are       
                          targets for bulk        
                          actions.                

  Bulk move               Move selected tabs      Chrome
                          together.               

  Bulk close              Close selected tabs.    Chrome

  Bulk mute               Mute selected tabs      Chrome /
                          where the action is     version-dependent
                          available.              

  Bulk bookmark           Bookmark selected tabs. Chrome /
                                                  version-dependent

  Group selected tabs     Put selected tabs into  Chrome
                          a new or existing       
                          group.                  

  Clear selection         Return to a single-tab  Chrome
                          selection by clicking   
                          an unselected tab or    
                          using the relevant      
                          interaction.            

  Invert selection        Select every unselected Custom / optional
                          tab and deselect the    
                          current selection.      

  Click-and-drag marquee  Select tabs by drawing  Custom / optional
  selection               a rectangle over the    
                          tab strip.              

  Preserve selection      Keep the                Chrome behavior /
  while opening a menu    multi-selection when    version-dependent
                          right-clicking a        
                          selected tab.           
  -----------------------------------------------------------------------

**Important terminology**

-   **Active tab:** the tab whose page is currently displayed.
-   **Selected tabs:** the tabs targeted by multi-tab operations.
-   A selected tab does not necessarily have to be the active tab.

For a custom implementation, keep `activeTabId` and `selectedTabIds` as
separate state variables.

## 4. Tab context menu (right-click menu)

Chrome's tab context menu changes according to the clicked tab's state
(for example, whether it is pinned or grouped) and the selected tabs.
The exact wording and ordering can change between Chrome releases.

### 4.1 Common tab actions

  -----------------------------------------------------------------------
  Menu action             What it does            Status
  ----------------------- ----------------------- -----------------------
  New tab                 Open a new tab.         Chrome

  Reload                  Reload the tab.         Chrome

  Duplicate               Create a duplicate tab. Chrome

  Pin tab                 Pin the tab at the      Chrome
                          beginning of the tab    
                          strip.                  

  Unpin tab               Return a pinned tab to  Chrome
                          normal.                 

  Mute site               Mute audio from the     Chrome
                          website.                

  Unmute site             Restore audio from the  Chrome
                          website.                

  Close tab               Close the current tab.  Chrome

  Reopen closed tab       Restore a recently      Chrome / menu and
                          closed tab.             history dependent
  -----------------------------------------------------------------------

**Chrome terminology note:** Chrome commonly labels the audio action
**Mute site** / **Unmute site**, rather than a strictly tab-local mute.
Site muting can affect other tabs from the same origin/site.

### 4.2 Closing actions

  -----------------------------------------------------------------------
  Menu action             What it does            Status
  ----------------------- ----------------------- -----------------------
  Close tab               Close the clicked tab.  Chrome

  Close other tabs        Close other tabs in the Chrome
                          window.                 

  Close tabs to the right Close tabs positioned   Chrome
                          after the clicked tab.  

  Close selected tabs     Close the current       Chrome /
                          multi-selection.        version-dependent

  Close group             Close all tabs in the   Chrome
                          group.                  

  Close all tabs          Close every tab in the  Custom menu action /
                          current window.         platform-dependent

  Close duplicate tabs    Find and close tabs     Custom / optional
                          with duplicate URLs.    

  Close inactive tabs     Close tabs that have    Custom / optional
                          not been used recently. 
  -----------------------------------------------------------------------

### 4.3 Grouping and organization actions

  -----------------------------------------------------------------------
  Menu action             What it does            Status
  ----------------------- ----------------------- -----------------------
  Add tab to new group    Create a group          Chrome
                          containing the tab.     

  Add tab to existing     Add the tab to a group. Chrome
  group                                           

  Remove from group       Ungroup the tab without Chrome
                          closing it.             

  New tab in group        Create a tab inside a   Chrome
                          group.                  

  Rename group            Change the group name.  Chrome

  Change group color      Choose a group color.   Chrome

  Collapse group          Hide the group's        Chrome
                          individual tabs while   
                          retaining its group     
                          label.                  

  Expand group            Show the group's tabs   Chrome
                          again.                  

  Save group              Save a tab group for    Chrome /
                          later use, where        version-dependent
                          supported and enabled.  

  Ungroup group           Remove the group while  Chrome
                          keeping its tabs open.  

  Close group             Close all tabs in the   Chrome
                          group.                  
  -----------------------------------------------------------------------

### 4.4 Moving actions

  -----------------------------------------------------------------------
  Menu action             What it does            Status
  ----------------------- ----------------------- -----------------------
  Move left / right       Move a tab by one       Custom / optional
                          position.               

  Move to beginning / end Reposition a tab in the Custom / optional
                          strip.                  

  Move to new window      Detach the tab into a   Chrome through
                          new window.             drag-and-drop; menu
                                                  entry may vary

  Move to another window  Transfer the tab to     Chrome through
                          another open window.    drag-and-drop; menu
                                                  entry may vary

  Move group to new       Transfer a group and    Chrome /
  window                  its tabs.               version-dependent
  -----------------------------------------------------------------------

### 4.5 Saving and sharing actions

  -----------------------------------------------------------------------
  Menu action             What it does            Status
  ----------------------- ----------------------- -----------------------
  Bookmark tab            Add the current page to Chrome through bookmark
                          bookmarks.              controls; not always a
                                                  tab-menu item

  Bookmark all tabs       Save all open tabs into Chrome
                          a bookmark folder.      

  Copy link address       Copy the page URL.      Chrome through
                                                  page/link context
                                                  menus; not always a
                                                  tab-menu item

  Send to your devices    Send the page to        Chrome / account and
                          another signed-in       device dependent
                          device.                 

  Share                   Open available sharing  Chrome / platform
                          options.                dependent

  Save to reading list    Save the page for       Chrome /
                          later.                  version-dependent
  -----------------------------------------------------------------------

### 4.6 Developer and advanced actions

  -----------------------------------------------------------------------
  Action                  What it does            Status
  ----------------------- ----------------------- -----------------------
  Inspect                 Open DevTools for the   Chrome through page
                          page or selected        context menu
                          element, depending on   
                          context.                

  View page source        Open the page's HTML    Chrome through page
                          source.                 context menu

  Open in incognito       Open the page in an     Custom shortcut/action;
                          Incognito window.       not a standard tab-menu
                                                  item

  Open in split view      Display two pages side  Chrome /
                          by side.                version-dependent

  Suspend tab             Explicitly suspend the  Not a standard Chrome
                          tab.                    tab-menu action

  Keep tab active         Exclude a tab from      Custom / optional
                          automatic resource      
                          management.             

  Tab properties          Display internal tab    Custom / optional
                          information.            
  -----------------------------------------------------------------------

### 4.7 Recommended context-menu behavior for a custom browser

1.  Right-clicking a selected tab should preserve the selection and
    apply bulk actions to it.
2.  Right-clicking an unselected tab should make it the sole selection,
    or clearly expose actions for only that tab.
3.  Show only actions that make sense for the clicked tab's state.
4.  Pinned tabs should expose **Unpin**, not **Pin**.
5.  Grouped tabs should expose group-related actions.
6.  Disable actions that are currently unavailable.
7.  Separate destructive actions from ordinary actions.
8.  Support keyboard navigation and `Shift+F10`.
9.  Keep menu placement inside the visible window.
10. Use submenus for long action categories rather than creating one
    enormous menu.

## 5. Tab groups

Chrome tab groups let users organize related tabs under a named, colored
group. Groups can be collapsed or expanded, and saved groups are
available in supported Chrome versions.

  -----------------------------------------------------------------------
  Feature                 What it does            Status
  ----------------------- ----------------------- -----------------------
  Create group            Create a group from one Chrome
                          or more tabs.           

  Name group              Assign a custom group   Chrome
                          name.                   

  Group color             Assign a predefined     Chrome
                          color.                  

  Collapse group          Hide the group's tabs   Chrome
                          behind its label.       

  Expand group            Show all tabs in the    Chrome
                          group.                  

  Move group              Reorder the group in    Chrome
                          the tab strip.          

  Drag tabs into group    Add tabs by dragging    Chrome
                          them into the group.    

  Drag tabs out of group  Remove tabs from the    Chrome
                          group.                  

  Add new tab to group    Open a tab within a     Chrome
                          group.                  

  Remove tab from group   Ungroup a tab.          Chrome

  Select group tabs       Select tabs in the      Chrome /
                          group for bulk actions. version-dependent

  Close group             Close all tabs in the   Chrome
                          group.                  

  Ungroup all tabs        Remove the group while  Chrome
                          retaining its tabs.     

  Save group              Keep a group available  Chrome /
                          for later, including    version-dependent
                          after closing it, where 
                          supported.              

  Restore saved group     Reopen a saved group.   Chrome /
                                                  version-dependent

  Sync saved groups       Make saved groups       Chrome / account
                          available across        dependent
                          signed-in devices where 
                          supported.              

  Move group to another   Transfer the group.     Chrome /
  window                                          version-dependent

  Group preview           Show group contents in  Custom / optional
                          a custom preview.       

  Nested groups           Create groups inside    Custom / optional
                          groups.                 

  Group-specific homepage Configure a homepage    Custom / optional
                          for each group.         

  Share group             Share a group with      Not a standard
  collaboratively         other users.            general-purpose Chrome
                                                  tab-group feature
  -----------------------------------------------------------------------

### Recommended group context menu

-   New tab in group
-   Rename group
-   Change color
-   Collapse group / Expand group
-   Select tabs in group
-   Save group
-   Move group to new window
-   Ungroup
-   Close group

## 6. Pinned tabs

  -----------------------------------------------------------------------
  Feature                 What it does            Status
  ----------------------- ----------------------- -----------------------
  Pin tab                 Keep a tab at the       Chrome
                          beginning of the tab    
                          strip in compact form.  

  Unpin tab               Restore its normal tab  Chrome
                          appearance.             

  Compact pinned          Usually show the        Chrome
  appearance              favicon rather than the 
                          full title.             

  Reorder pinned tabs     Change the order of     Chrome
                          pinned tabs.            

  Move pinned tab to      Transfer a pinned tab.  Chrome /
  another window                                  version-dependent

  Restore pinned tabs on  Reopen tabs from the    Chrome
  startup                 previous session when   
                          startup settings allow  
                          it.                     

  Pin multiple tabs       Pin tabs individually;  Chrome /
                          bulk support may vary.  version-dependent

  Pin an entire group     Keep a whole group in a Custom / optional
                          special pinned state.   

  Prevent accidental      Require confirmation or Custom / optional
  closure                 protect pinned tabs.    

  Lock pinned tab to a    Prevent navigation away Custom / optional
  specific site           from its original site. 
  -----------------------------------------------------------------------

## 7. Audio, video and permission indicators

  -----------------------------------------------------------------------
  Feature                 What it does            Status
  ----------------------- ----------------------- -----------------------
  Audio indicator         Show that a page is     Chrome
                          playing sound.          

  Mute site               Mute audio from the     Chrome
                          site.                   

  Unmute site             Restore site audio.     Chrome

  Video playback          Identify tabs playing   Chrome / UI dependent
  indicator               video.                  

  Picture-in-Picture      Detach supported video  Chrome
                          into a floating window. 

  Media controls          Control supported media Chrome / platform
                          playback from browser   dependent
                          UI.                     

  Microphone-use          Show when a site uses   Chrome
  indicator               the microphone.         

  Camera-use indicator    Show when a site uses   Chrome
                          the camera.             

  Screen-sharing          Show when a site is     Chrome
  indicator               sharing or capturing    
                          the screen.             

  Bulk mute selected tabs Mute several tabs at    Chrome /
                          once.                   version-dependent

  Mute all other tabs     Mute tabs other than    Custom / optional
                          the current one.        
  -----------------------------------------------------------------------

## 8. Tab appearance and visual states

### 8.1 Tab states

  -----------------------------------------------------------------------
  State                   Suggested               Status
                          representation          
  ----------------------- ----------------------- -----------------------
  Normal                  Standard tab            Chrome
                          background.             

  Active                  Highlighted background  Chrome
                          or border.              

  Hovered                 Hover highlight and     Chrome
                          optional preview.       

  Selected                Distinct selection      Chrome
                          appearance.             

  Pinned                  Compact favicon tab.    Chrome

  Loading                 Spinner or loading      Chrome
                          animation.              

  Loaded                  Website favicon.        Chrome

  Audio playing           Audio indicator.        Chrome

  Muted                   Muted audio indicator.  Chrome

  Sleeping / discarded    Optional subdued state  Chrome / custom
                          or tooltip.             visibility

  Crashed                 Error indication.       Chrome

  Grouped                 Group color accent.     Chrome

  Private                 Incognito identity is   Chrome
                          represented at window   
                          level.                  

  Unsaved changes         App-specific indicator. Custom / app-dependent
  -----------------------------------------------------------------------

### 8.2 Appearance customization

  -----------------------------------------------------------------------
  Feature                 What it does            Status
  ----------------------- ----------------------- -----------------------
  Light / dark theme      Change browser          Chrome
                          appearance.             

  Custom theme            Change browser colors   Chrome
                          and background.         

  Tab height              Adjust tab strip        Custom / optional
                          height.                 

  Tab width               Configure minimum and   Custom / optional
                          maximum widths.         

  Rounded / square        Change tab shape.       Theme / custom
  corners                                         

  Tab spacing             Change distance between Custom / optional
                          tabs.                   

  Compact mode            Reduce tab dimensions.  Custom / optional

  Favicon-only tabs       Hide titles and show    Custom / optional
                          icons only.             

  Show / hide close       Configure close-button  Custom / optional
  buttons                 visibility.             

  Custom group colors     Choose group colors     Chrome
                          from Chrome's available 
                          palette.                

  Custom favicon size     Change favicon          Custom / optional
                          dimensions.             

  Tab animations          Configure transitions   Custom / optional
                          and effects.            

  Active-tab indicator    Change active tab       Theme / custom
                          highlight.              

  Hover effects           Customize hover         Theme / custom
                          appearance.             

  Tab bar background      Customize the browser   Chrome themes
                          frame theme.            

  Custom separators /     Change tab boundaries.  Theme / custom
  borders                                         
  -----------------------------------------------------------------------

### 8.3 Tab hover cards and previews

Chrome's tab hover cards can show a page title and preview image,
depending on settings and version.

Potential information for an extended custom preview:

-   Page title
-   Website URL
-   Favicon
-   Page thumbnail
-   Loading state
-   Audio state
-   Group name
-   Last active time
-   Memory usage (if available)
-   CPU usage (if available)

Recommended UX:

-   Add a configurable hover delay.
-   Avoid showing previews immediately when the pointer crosses the tab
    strip.
-   Keep the preview near the hovered tab without covering important
    controls.
-   Do not make the preview the only way to access tab information.

## 9. Tab overflow, search and navigation

  -----------------------------------------------------------------------
  Feature                 What it does            Status
  ----------------------- ----------------------- -----------------------
  Horizontal tab-strip    Reach tabs that do not  Chrome
  navigation              fit in the visible      
                          strip.                  

  Tab search              Search open tabs by     Chrome
                          title or URL.           

  Tab switcher            Display a list of open  Chrome
                          tabs.                   

  Jump to tab             Activate a result from  Chrome
                          tab search.             

  Recently closed tabs    Find recently closed    Chrome
                          pages through History   
                          or restore controls.    

  Search across windows   Find tabs in other      Chrome /
                          Chrome windows where    version-dependent
                          supported by the        
                          tab-search UI.          

  Tab counter             Display the number of   Custom / optional
                          open tabs.              

  Group counter           Display the number of   Custom / optional
                          tabs in a group.        

  Sort alphabetically     Sort tabs by title.     Custom / optional

  Sort by URL / domain    Organize tabs by        Custom / optional
                          website.                

  Sort by recently used   Order tabs by last      Custom / optional
                          activation.             

  Sort by creation date   Order tabs by opening   Custom / optional
                          time.                   

  Detect duplicate URLs   Identify duplicate      Custom / optional
                          pages.                  

  Search within a group   Limit tab search to one Custom / optional
                          group.                  
  -----------------------------------------------------------------------

### Useful keyboard navigation

  -----------------------------------------------------------------------
  Shortcut (Windows / Linux)          Action
  ----------------------------------- -----------------------------------
  `Ctrl+Tab`                          Next tab

  `Ctrl+Shift+Tab`                    Previous tab

  `Ctrl+1` to `Ctrl+8`                Activate tab by position

  `Ctrl+9`                            Activate last tab

  `Ctrl+T`                            Open new tab

  `Ctrl+W`                            Close current tab

  `Ctrl+Shift+T`                      Reopen last closed tab

  `Ctrl+Page Up`                      Previous tab

  `Ctrl+Page Down`                    Next tab

  `Ctrl+L`                            Focus address bar

  `Ctrl+R`                            Reload current tab

  `Ctrl+Shift+R`                      Hard reload, where supported by the
                                      browser UI

  `Ctrl+D`                            Bookmark current page

  `Ctrl+Shift+D`                      Bookmark all open tabs

  `Ctrl+N`                            New window

  `Ctrl+Shift+N`                      New Incognito window

  `Shift+F10`                         Open context menu for the focused
                                      UI item, where applicable
  -----------------------------------------------------------------------

**Shortcut note:** macOS generally uses `Command` in place of `Ctrl` for
browser shortcuts. Some shortcuts vary by operating system and Chrome
release. Avoid assigning custom shortcuts that conflict with established
browser shortcuts.

### Optional custom shortcuts

  Suggested shortcut   Action
  -------------------- ----------------------------
  `Ctrl+Shift+A`       Search all tabs
  `Ctrl+Shift+G`       Create a tab group
  `Ctrl+Shift+P`       Pin / unpin current tab
  `Ctrl+Shift+M`       Mute / unmute current site
  `Ctrl+Shift+C`       Close other tabs
  `Ctrl+Shift+X`       Close tabs to the right
  `Ctrl+Shift+S`       Save current session

These are suggestions for a custom browser, not standard Chrome
shortcuts.

## 10. Session management and recovery

  -----------------------------------------------------------------------
  Feature                 What it does            Status
  ----------------------- ----------------------- -----------------------
  Continue where you left Restore the previous    Chrome
  off                     session at startup.     

  Reopen closed tab       Restore a recently      Chrome
                          closed tab.             

  Recently closed list    Access recently closed  Chrome
                          tabs and windows        
                          through History.        

  Restore closed window   Reopen a recently       Chrome
                          closed window where     
                          available in history.   

  Restore closed group    Reopen a saved group or Chrome /
                          recently closed group   version-dependent
                          where supported.        

  Crash recovery          Offer restoration after Chrome
                          an unexpected shutdown. 

  Save bookmarks for all  Store open tabs in a    Chrome
  tabs                    bookmark folder.        

  Save group              Keep a group for later  Chrome /
                          use.                    version-dependent

  Session snapshots       Save the complete tab   Custom / optional
                          and window layout as a  
                          named snapshot.         

  Export session          Export tabs, groups and Custom / optional
                          layout.                 

  Import session          Restore an exported     Custom / optional
                          session.                

  Session history         Browse named historical Custom / optional
                          snapshots.              

  Restore selected tabs   Choose which tabs to    Custom / optional
  only                    restore.                

  Restore into a new      Restore a saved session Custom / optional
  window                  separately.             

  Automatic periodic      Save the current state  Custom / optional
  session saves           on a schedule.          

  Sync session state      Synchronize tabs across Chrome / account
                          devices through the     dependent
                          signed-in browser       
                          account.                
  -----------------------------------------------------------------------

## 11. Performance and memory management

Chrome manages background tabs automatically and includes Memory Saver
controls. The precise controls and labels can vary by release.

  -----------------------------------------------------------------------
  Feature                 What it does            Status
  ----------------------- ----------------------- -----------------------
  Background tab          Limit some background   Chrome
  throttling              activity to reduce      
                          resource use.           

  Memory Saver            Deactivate eligible     Chrome
                          inactive tabs to free   
                          memory.                 

  Automatic tab discard   Release resources from  Chrome
                          tabs when necessary.    

  Reload discarded tab    Reload a discarded page Chrome
                          when selected.          

  Memory Saver exceptions Keep selected sites     Chrome
                          active using the        
                          exception list.         

  Manual discard          Discard a tab through   Chrome / advanced
                          Chrome's internal       
                          task-management tools   
                          where available.        

  Automatic suspension    Suspend tabs after a    Custom / optional
  timer                   user-defined inactive   
                          period.                 

  Suspend selected tabs   Manually suspend        Custom / optional
                          selected tabs.          

  Suspend all other tabs  Suspend all eligible    Custom / optional
                          tabs except the active  
                          one.                    

  Per-tab memory display  Display memory use for  Chrome / task manager
                          individual tabs.        or hover UI,
                                                  version-dependent

  Per-tab CPU display     Display CPU             Chrome Task Manager
                          consumption.            

  Last active timestamp   Show how long a tab has Custom / optional
                          been inactive.          

  Battery optimization    Reduce background       Chrome / platform
                          resource use.           dependent

  Keep tab active         Exclude a tab from      Chrome through Memory
                          automatic suspension.   Saver exceptions /
                                                  custom UI
  -----------------------------------------------------------------------

**Terminology**

-   **Throttling:** reduces the frequency or intensity of background
    work.
-   **Sleeping / deactivation:** pauses or limits background activity
    while retaining a recoverable tab.
-   **Discarding:** releases the page's renderer resources; selecting
    the tab generally causes it to reload.

Do not promise that every sleeping or discarded tab can resume without
reloading. Some web applications lose transient state when their page is
discarded.

## 12. Multiple windows and cross-window operations

  -----------------------------------------------------------------------
  Feature                 What it does            Status
  ----------------------- ----------------------- -----------------------
  Detach tab into new     Drag a tab away from    Chrome
  window                  its current window.     

  Move tab between        Drag it into another    Chrome
  windows                 Chrome window.          

  Move selected tabs      Transfer a              Chrome /
                          multi-selection.        version-dependent

  Move group between      Transfer a group.       Chrome /
  windows                                         version-dependent

  Merge windows           Move tabs from one      Chrome through
                          window into another.    drag-and-drop

  Window tab overview     Inspect tabs belonging  Chrome through tab
                          to a window.            search / UI

  Search tabs across      Find tabs in multiple   Chrome /
  windows                 windows.                version-dependent

  Window-specific session Save and restore a      Custom / optional
  snapshots               window's layout.        

  Split windows           Display separate        Operating-system
                          browser windows side by feature / custom
                          side.                   browser integration
  -----------------------------------------------------------------------

## 13. Split view and side-by-side browsing

Chrome has introduced split-view capabilities in some desktop versions
and rollouts. Availability and exact behavior can vary.

Potential feature set:

-   Open two tabs side by side.
-   Add a tab to a split view.
-   Drag a tab to the left or right side.
-   Adjust the split ratio, if supported.
-   Swap the two pages.
-   Close one side of the split.
-   Exit split view.
-   Restore the tabs to the normal tab strip.
-   Save a split-view arrangement.
-   Create layouts with more than two panels.

Features such as three- or four-panel layouts, synchronized scrolling,
named layouts and persistent split-view workspaces are custom / optional
unless explicitly supported by the target Chrome version.

## 14. Workspaces and advanced organization

Chrome's tab groups provide a useful basis for organizing browsing. The
following go beyond ordinary Chrome tab-group functionality and are
ideas for an extended browser.

  -----------------------------------------------------------------------
  Feature                 What it does            Status
  ----------------------- ----------------------- -----------------------
  Workspaces              Separate browsing tasks Custom / optional
                          into independent        
                          collections of tabs.    

  Switch workspace        Change the active       Custom / optional
                          collection.             

  Workspace color / icon  Identify workspaces     Custom / optional
                          visually.               

  Workspace persistence   Restore workspaces      Custom / optional
                          after restarting.       

  Workspace-specific      Organize groups inside  Custom / optional
  groups                  each workspace.         

  Workspace-specific      Maintain different      Custom / optional
  pinned tabs             pinned tabs per         
                          workspace.              

  Temporary tabs          Open tabs that are      Custom / optional
                          automatically closed    
                          later.                  

  Automatic tab close     Close inactive tabs     Custom / optional
                          after a configurable    
                          period.                 

  Tab expiration          Set a date or time for  Custom / optional
                          automatic closure.      

  Tab locking             Prevent accidental      Custom / optional
                          navigation or closure.  

  Tab protection          Exclude selected tabs   Custom / optional
                          from bulk-close         
                          actions.                

  Tab notes               Attach personal notes   Custom / optional
                          to a tab.               

  Tab tags                Add searchable labels   Custom / optional
                          to tabs.                

  Tab collections         Save multiple tabs as a Custom / optional
                          named collection.       

  Automatic organization  Suggest groups based on Custom / optional
                          domain, title or        
                          browsing context.       
  -----------------------------------------------------------------------

## 15. Tab layout modes

Chrome's standard desktop interface uses a horizontal tab strip. The
following alternatives are useful for a custom browser, but should not
be presented as standard Chrome features.

  -----------------------------------------------------------------------
  Layout                  Description             Status
  ----------------------- ----------------------- -----------------------
  Horizontal tabs         Traditional top tab     Chrome
                          strip.                  

  Vertical tabs           Tabs displayed in a     Not standard Chrome
                          sidebar.                desktop UI; custom /
                                                  optional

  Collapsible vertical    Sidebar expands on      Custom / optional
  tabs                    hover.                  

  Multi-row tabs          Display tabs across     Custom / optional
                          several rows.           

  Two-level tabs          Show groups on one row  Custom / optional
                          and their tabs on       
                          another.                

  Compact tabs            Reduce tab width and    Custom / optional
                          spacing.                

  Favicon-only tabs       Hide titles and show    Custom / optional
                          icons only.             

  Hidden tab strip        Hide tabs until         Custom / optional
                          requested.              

  Fullscreen reveal       Show the tab strip when Custom / optional
                          hovering near the top   
                          edge.                   
  -----------------------------------------------------------------------

## 16. Accessibility and usability

-   Full keyboard navigation.
-   Screen-reader support and accessible tab names.
-   Accessible group names.
-   Visible focus indicators.
-   High-contrast support.
-   Reduced-motion support.
-   Configurable animations.
-   Adjustable text size where appropriate.
-   Tooltips for icon-only controls.
-   Adequately sized clickable targets.
-   Clear distinction between active and selected tabs.
-   Keyboard-accessible context menus.
-   Keyboard alternatives to drag-and-drop.
-   Do not rely on color alone to distinguish groups.
-   Confirmation before closing protected tabs.
-   Undo support for accidental actions.
-   Logical focus restoration after closing or moving a tab.
-   Predictable focus behavior when collapsing or expanding groups.

## 17. Suggested context-menu layout

A practical custom context menu can group actions into sections instead
of presenting every possible action in one long list.

``` text
[Site favicon]  Example page
--------------------------------
New tab
Reload
Duplicate tab
Pin tab
Mute site
--------------------------------
Add to tab group       >
Move tab               >
Bookmark tab
Copy URL
Send to your devices   >
--------------------------------
Select all tabs
Close tabs to the right
Close duplicate tabs
Close other tabs
Close tab
```

### Suggested submenus

**Add to tab group** - New group - Existing group A - Existing group B -
Existing group C - Remove from group

**Move tab** - Move left - Move right - Move to beginning - Move to
end - Move to new window - Move to another window

**Close tabs** - Close selected tabs - Close other tabs - Close tabs to
the right - Close tabs to the left - Close duplicate tabs - Close all
tabs

**Send to device** - Device A - Device B - Device C

> **Oxide note.** There is no **Move tab** submenu. Tabs are reordered by
> dragging them inside the strip, and dragging a tab out of the strip tears
> it off into a new window - see *Dragging, dropping and reordering tabs*.

### Context-menu rules

1.  Right-clicking a selected tab preserves the selection.
2.  Right-clicking an unselected tab should make it the sole selection,
    unless a modifier is used to preserve the existing selection.
3.  Bulk operations apply to all selected tabs where appropriate.
4.  Pinned tabs should be protected from bulk-close operations by
    default, or the user should receive a clear confirmation.
5.  Hide or disable actions that do not apply to the current tab.
6.  Separate destructive actions visually.
7.  Support keyboard navigation and accessible focus.
8.  Keep submenus usable with both mouse and keyboard.

------------------------------------------------------------------------

## 18. Recommended implementation priorities

### Phase 1 --- Core tab functionality

-   [ ] Open, close and activate tabs.
-   [ ] Drag and reorder tabs.
-   [ ] Detach tabs into new windows.
-   [ ] Pin and unpin tabs.
-   [ ] Basic tab context menu.
-   [ ] Reload and duplicate tabs.
-   [ ] Loading and favicon indicators.
-   [ ] Standard keyboard shortcuts.
-   [ ] Tab overflow and scrolling.
-   [ ] Restore recently closed tabs.

### Phase 2 --- Chrome-style tab organization

-   [ ] Multi-selection with Ctrl/Cmd and Shift.
-   [ ] Bulk tab operations.
-   [ ] Tab groups.
-   [ ] Group names and colors.
-   [ ] Collapse and expand groups.
-   [ ] Group context menu.
-   [ ] Move tabs between windows.
-   [ ] Tab search.
-   [ ] Save and restore tab groups where supported.
-   [ ] Bookmark all tabs.

### Phase 3 --- Advanced browser features

-   [ ] Split view.
-   [ ] Memory Saver / tab discard integration.
-   [ ] Tab hover cards and previews.
-   [ ] Cross-window tab search.
-   [ ] Session restoration.
-   [ ] Workspace support.
-   [ ] Advanced tab protection.
-   [ ] Vertical tabs (optional alternative layout).
-   [ ] Customizable tab appearance.
-   [ ] Cross-device synchronization.

### Phase 4 --- Power-user features

-   [ ] Nested tab groups.
-   [ ] Tab tags and notes.
-   [ ] Automatic tab organization.
-   [ ] Custom tab layouts.
-   [ ] Named session snapshots.
-   [ ] Tab memory and CPU monitoring.
-   [ ] Automatic tab expiration.
-   [ ] Customizable shortcuts.
-   [ ] Tab collections.
-   [ ] Advanced bulk actions.

------------------------------------------------------------------------

## 19. Suggested tab-system architecture

Avoid implementing the tab bar as a simple visual list of webpage
widgets. Keep the tab model, selection model, group model and browser
contents separate.

``` text
BrowserWindow
├── TabBar
│   ├── PinnedTabs[]
│   ├── NormalTabs[]
│   ├── TabGroups[]
│   │   ├── GroupId
│   │   ├── GroupName
│   │   ├── GroupColor
│   │   ├── Collapsed
│   │   └── TabIds[]
│   └── TabSelection
│       ├── ActiveTabId
│       └── SelectedTabIds[]
│
├── WebContents[]
│   ├── TabId
│   ├── URL
│   ├── PageTitle
│   ├── Favicon
│   ├── LoadingState
│   ├── AudioState
│   └── NavigationHistory
│
├── WorkspaceManager
├── SessionManager
└── WindowManager
```

### Architecture recommendations

-   Give every tab a stable unique ID.
-   Keep `activeTabId` separate from `selectedTabIds`.
-   Keep tab order separate from the underlying webpage instances.
-   Represent group membership explicitly.
-   Treat pinned tabs as a distinct organizational state.
-   Make bulk actions operate on stable tab IDs, not visual indices.
-   Preserve navigation history when moving a tab between windows.
-   Define how groups behave when moved, duplicated or closed.
-   Make context-menu contents depend on the current selection and tab
    state.
-   Save session state independently from live webpage objects.
-   Make window and workspace ownership explicit.
-   Ensure asynchronous page events cannot update a tab that has already
    been closed.

------------------------------------------------------------------------

## 20. Chrome references

Use official Chrome documentation to verify behavior against the version
and platform being targeted.

-   [Google Chrome Help --- Manage tabs in
    Chrome](https://support.google.com/chrome/answer/2391819)
-   [Google Chrome Help --- Use tab groups with
    Chrome](https://support.google.com/chrome/answer/2391819)
-   [Google Chrome Help --- Reopen recently closed
    tabs](https://support.google.com/chrome/answer/2391819)
-   [Google Chrome Help --- Speed up Google
    Chrome](https://support.google.com/chrome/answer/1385029)
-   [Google Chrome Help Center](https://support.google.com/chrome/)

**Implementation note:** Chrome is updated frequently and some
capabilities are rolled out gradually. Verify menu labels, shortcuts,
split-view availability, saved-group behavior and platform differences
in the exact Chrome release used as the reference.

------------------------------------------------------------------------

## Final recommendation

For a Chrome-like browser, the most important features to get right are:

1.  **Multi-selection** --- Ctrl/Cmd-click, Shift-click and reliable
    bulk actions.
2.  **Drag-and-drop** --- reorder tabs, move selected tabs, detach tabs
    and move tabs into groups.
3.  **Tab groups** --- names, colors, collapse/expand, context menu and
    saved groups.
4.  **Context menus** --- clear actions that adapt to pinned, grouped
    and selected tabs.
5.  **Session recovery** --- restore closed tabs, windows and previous
    sessions.
6.  **Tab search** --- quickly find tabs across a large number of open
    pages.
7.  **Memory management** --- background throttling, Memory Saver and
    safe tab discard.
8.  **Cross-window management** --- move tabs and groups without losing
    their state.

A good implementation should first reproduce Chrome's familiar behavior
accurately, then add optional power-user features such as workspaces,
vertical tabs, tab notes, nested groups and custom session snapshots.
