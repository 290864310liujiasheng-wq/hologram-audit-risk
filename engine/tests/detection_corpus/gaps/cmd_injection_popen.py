import os
output = os.popen("grep " + pattern + " /var/log/app.log").read()
