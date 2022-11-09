# Resilient Components

This library provides supervision of components that should be consistently running without downtime.
We automatically restart a component when it stops running, and provide a contract for the component to
communicate with the supervisor to signal when a situation arises that requires a restart.


## Use

For each component that needs to be registered with supervision, we will instantiate a `Supervisor` object to do supervision for that component.
The supervisor should be initialized by passing in an object that implements the `Manageable` trait. To start the component,
call the spawn method and await.
```asm
 let supervisor = Supervisor::new(stream_component);

 supervisor.spawn().await?
```

A user-provided async function will be called by the supervisor to start a tokio task. We wrap the tokio task with communication channels, and one end of the channels will be passed into the user-provided function which will run the component, and the other ends of the channels will be held by the supervisor to receive irrecoverable error messages and to send cancellation signals to the tokio task.

In order to have a component that gets managed, a user needs to implement the following async functions on their struct to be implementing the Manageable trait:

1. start - This function is responsible for launching the tokio task that needs parental supervision and panic-tolerance. It will have as input a channel sender for passing irrecoverable errors to the supervisor, as well as a channel receiver that will be listened to after sending a message to the supervisor to receive an ack that the message was received. The function should then return on reception of the cancellation signal.
2. handle_irrecoverable - This function will be called upon receiving an irrecoverable error. This allows the user to decide what to log or alert, and what actions to take, if any, for this particular component in terms of resource cleanup. After this function is called, the component is restarted again via the start function above.

After these functions are implemented, a user can create a Supervisor object with the constructor and then call spawn on the supervisor. This will run the supervision on the component and will consistently ensure that it restarted after any irrecoverable errors.

Note that since we never expect these tasks to complete, the user does not need to call join on these handles created in the start function. The constantly running supervision of the task will also handle the event that the async task execution completes by ending supervision, although this is not an expected use case.


## Example

The executable example is used as an integration test for this library. To run

You can run it with
```
cargo run --example stream_example
```
and then ctrl + c to stop the component.

This should generate the following output.
```
starting component task
Received irrecoverable error: missing something required
starting component task
terminating component task
```
