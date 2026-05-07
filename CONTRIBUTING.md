# Syncbus Contribution Guide

Thanks for taking time to contribute to Syncbus! Here are some suggestions we
recommend to keep in mind to make the journey easier.

Syncbus is part of [Phosh ecosystem](https://phosh.mobi), so much of our
policies are derived from it. Therefore, it is recommended to give [Phosh
Contributors Manual](https://dev.phosh.mobi/) a read.

## Workflow

We follow a merge request based workflow. To contribute changes, please fork the
repository, create a new branch, add your changes and create a merge request to
`main` of our repository.

The commit history must follow the _recipe style_ rather than being a work log.
Please read [Git history: work log vs
recipe](https://www.bitsnbites.eu/git-history-work-log-vs-recipe/) to understand
the difference.

Commit messages must follow the following template.

```
component: Changes to the component

Optional description explaining the changes.

Closes: <URL to issue this commit fixes, if applicable>
Fixes: <Commit SHA of the earlier commit this commit fixes, if applicable>

Signed-off-by: Name <email>
```

In the above template, `component` refers to the fundamental unit whose changes
are relevant.

On the opened merge request, please feel free to add information to understand
the changes better. This includes adding CLI output, screenshots, videos etc.

If the changes are not ready to be merged and you think an early review would be
better (for example to know if the strategy you used is right), then please feel
free to open a merge request and mark it as _draft_.

After every major changes to your merge request, adding a comment that
summarizes the changes would be very much helpful in reviewing the code.

## Coding Style

We do not have a strong preference to style yet (functional versus object-oriented
etc.). So we instead suggest the contributor to use the coding pattern they feel the
best for the situation. This can be always reviewed and improved.

However, we do have automated lints for formatting and catching few
improvements; thanks to `rustfmt` and Clippy. Please stick to their suggestions
as far as possible.

To format your code, please run the following command.

```sh
cargo +nightly fmt --all
```

To lint your code, please run the following command.

```sh
cargo clippy --workspace --no-deps -- -D warnings
```

You can always adjust the arguments to optimize it for the situation.

## Conclusion

These guidelines are not meant to bring in a form of _red tapism_. So feel free
to go ahead with your contribution as we can always make reasonable changes as
required. If you are stuck and think we could help, you are welcome to add it as
a comment and we will do our best to resolve it.
