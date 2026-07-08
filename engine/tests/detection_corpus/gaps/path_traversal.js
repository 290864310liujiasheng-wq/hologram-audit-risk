const fs = require('fs');
app.get('/file', (req, res) => res.send(fs.readFileSync('./uploads/' + req.query.name)));
