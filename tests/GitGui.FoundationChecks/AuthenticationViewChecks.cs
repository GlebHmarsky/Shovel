using System.IO;
using System.Windows;
using System.Windows.Automation;
using System.Windows.Controls;
using System.Windows.Threading;
using GitGui.App.Localization;
using GitGui.App.Services;
using GitGui.App.ViewModels;
using GitGui.App.Views;
using GitGui.Core.Git;
using GitGui.Core.Processes;
using GitGui.ProcessHost;

namespace GitGui.FoundationChecks;

internal static partial class Program
{
    private static async Task RunAuthenticationViewChecksAsync()
    {
        await Check("Authentication prompts render offscreen with masked input, safe default and accessible localized controls", async () =>
        {
            foreach (var kind in Enum.GetValues<AuthenticationPromptKind>())
            {
                var window = new AuthenticationWindow("Fetch — fixture/repository", new(kind,
                    "The authenticity of host 'example.invalid' cannot be established.\nED25519 fingerprint is SHA256:fixture-only.\nAre you sure you want to continue connecting (yes/no/[fingerprint])?"));
                try
                {
                    var content = (FrameworkElement)window.Content;
                    Arrange(content, 620, 440); await Dispatcher.Yield(DispatcherPriority.ContextIdle);
                    SaveRender(content, "authentication-" + kind + ".png", 620, 440);
                    Arrange(content, 480, 340); await Dispatcher.Yield(DispatcherPriority.ContextIdle);
                    SaveRender(content, "authentication-" + kind + "-minimum.png", 480, 340);
                    var input = VisualDescendants<PasswordBox>(content).Single();
                    Expect(AutomationProperties.GetName(input) == UiText.Get("AuthResponse") && input.MaxLength == 1024, "Bounded, accessible masked response");
                    Expect(VisualDescendants<Button>(content).Single(button => button.IsDefault).Content.ToString() == UiText.Get("AuthCancel"), "Enter without deliberate input cannot approve a host or request");
                    Expect(window.TakeAnswer() is null, "No implicit affirmative response");
                }
                finally { window.Close(); }
            }
        });
        if (_actionFixture is null) throw new InvalidOperationException("Action fixture required");
        await Check("Authentication waiting/cancellation remains in the modal operation until repository reconciliation finishes", async () =>
        {
            var inner = new WindowsProcessRunner(); var reader = new RepositoryReader(inner); var identities = new WindowsRepositoryIdentity();
            var context = await new GitActions(inner, reader, identities).PrepareAsync(_actionFixture.Git, _actionFixture.Repository.Worktree, default);
            var runner = new PromptActionRunner(inner); var interaction = new PromptInteraction();
            var uiThread = Environment.CurrentManagedThreadId;
            var reconciled = 0;
            var model = new GitActionViewModel(context, GitAction.Push, new GitActions(runner, reader, identities),
                () => { reconciled++; return Task.CompletedTask; }, interaction: interaction) { Approved = true };
            model.RunCommand.Execute(null);
            await interaction.Started.Task.WaitAsync(TimeSpan.FromSeconds(10));
            Expect(interaction.SetupThread != uiThread, "Helper file/pipe setup stays off dispatcher");
            Expect(model.IsRunning && model.Status == UiText.Get("ActionWaitingAuthentication") && !model.CanEdit, "Explicit waiting state while navigation stays gated");
            model.Cancel();
            await model.RunningTask.WaitAsync(TimeSpan.FromSeconds(10));
            Expect(interaction.Closed && !model.IsRunning && reconciled == 1 && model.Status == UiText.Get("ActionResultCancelled"), "Prompt closed, result preserved, fresh reconciliation required");
            Expect(runner.PreparationNoninteractive, "Preparation reads never receive the prompt endpoint");
        });
    }
    private sealed class PromptInteraction : IUserInteraction
    {
        public TaskCompletionSource Started { get; } = new();
        public bool Closed { get; private set; }
        public int SetupThread { get; private set; }
        public string? ChooseRepository() => null;
        public string? ChooseGit() => null;
        public IAuthenticationSession CreateAuthenticationSession(string context,
            Func<AuthenticationPrompt, CancellationToken, Task<AuthenticationAnswer?>> prompt, CancellationToken token)
        {
            SetupThread = Environment.CurrentManagedThreadId;
            return new AskPassSession(AskPassExecutable, prompt, token);
        }
        public async Task<AuthenticationAnswer?> ShowAuthenticationAsync(string context, AuthenticationPrompt prompt, CancellationToken token)
        {
            Started.TrySetResult();
            try { await Task.Delay(Timeout.Infinite, token); return null; }
            finally { Closed = true; }
        }
    }
    private sealed class PromptActionRunner(IProcessRunner inner) : IProcessRunner
    {
        public bool PreparationNoninteractive { get; private set; } = true;
        public Task<ProcessResult> RunAsync(ProcessRequest request, CancellationToken token, Action<ProcessOutput>? output = null)
        {
            if (request.Arguments.Contains("push"))
            {
                Expect(request.Environment["GCM_INTERACTIVE"] == "never" && request.Environment["SSH_ASKPASS_REQUIRE"] == "force" &&
                    request.Environment["GIT_SSH_COMMAND"]!.Contains("StrictHostKeyChecking=ask"), "Only explicit write permits broker; host verification stays enabled");
                return inner.RunAsync(request with { Executable = AskPassExecutable, Arguments = ["fixture password"] }, token, output);
            }
            PreparationNoninteractive &= !request.Environment.TryGetValue("SHOVEL_ASKPASS_PIPE", out var value) || value is null;
            return inner.RunAsync(request, token, output);
        }
    }
}
