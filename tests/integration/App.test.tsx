import { Suspense, type ComponentType } from "react";
import { QueryClient, QueryClientProvider } from "@tanstack/react-query";
import { render, screen, waitFor, fireEvent } from "@testing-library/react";
import { describe, it, expect, beforeEach, vi } from "vitest";
import { resetProviderState, setSettings } from "../msw/state";
import userEvent from "@testing-library/user-event";

const toastSuccessMock = vi.fn();
const toastErrorMock = vi.fn();

vi.mock("sonner", () => ({
  toast: {
    success: (...args: unknown[]) => toastSuccessMock(...args),
    error: (...args: unknown[]) => toastErrorMock(...args),
  },
}));

const renderApp = (AppComponent: ComponentType) => {
  const client = new QueryClient({
    defaultOptions: {
      queries: {
        retry: false,
      },
    },
  });
  return render(
    <QueryClientProvider client={client}>
      <Suspense fallback={<div data-testid="loading">loading</div>}>
        <AppComponent />
      </Suspense>
    </QueryClientProvider>,
  );
};

describe("App integration with MSW", () => {
  beforeEach(() => {
    resetProviderState();
    toastSuccessMock.mockReset();
    toastErrorMock.mockReset();
  });

  it("shows first run wizard when not confirmed, and supports configuration flow", async () => {
    setSettings({ firstRunNoticeConfirmed: false });

    const { default: App } = await import("@/App");
    renderApp(App);

    await waitFor(() => {
      expect(screen.getByText("稍后配置")).toBeInTheDocument();
    });

    const skipButton = screen.getByText("稍后配置");
    fireEvent.click(skipButton);

    await waitFor(() => {
      expect(screen.queryByText("粘贴你的 Matpool Token")).not.toBeInTheDocument();
      expect(screen.getByText("Matpool Token")).toBeInTheDocument();
    });
  });

  it("renders main dashboard and handles CLI takeover toggle flows", async () => {
    setSettings({ firstRunNoticeConfirmed: true });

    const { default: App } = await import("@/App");
    renderApp(App);

    await waitFor(() => {
      expect(screen.getByText("Claude Code")).toBeInTheDocument();
      expect(screen.getByText("Codex")).toBeInTheDocument();
      expect(screen.getByText("Gemini CLI")).toBeInTheDocument();
      expect(screen.getByText(/sk-/)).toBeInTheDocument();
    });

    const switchButtons = screen.getAllByRole("switch");
    expect(switchButtons.length).toBe(3);

    await waitFor(() => {
      expect(switchButtons[0]).not.toBeDisabled();
    });

    await userEvent.click(switchButtons[0]);

    await waitFor(() => {
      expect(toastErrorMock).not.toHaveBeenCalled();
      expect(toastSuccessMock).toHaveBeenCalledWith("Claude Code 已接管");
    });

    await userEvent.click(switchButtons[0]);
    await waitFor(() => {
      expect(toastSuccessMock).toHaveBeenCalledWith("Claude Code 已取消接管");
    });
  });
});
