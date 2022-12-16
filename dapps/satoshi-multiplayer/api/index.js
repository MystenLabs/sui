const cors = require('cors');
const app = require('express')();
const http = require('http').Server(app);
const io = require('socket.io')(http, {
    cors: {
        origin: "*",
        methods: ["GET", "POST"],
        credentials: false
    }
});


const port = process.env.PORT || 8000;

io.on('connection', (socket) => {
  socket.onAny((ev, ...args) => {
    console.log(ev, args)
  });
});

http.listen(port, () => {
  console.log(`Socket.IO server running at http://localhost:${port}/`);
});