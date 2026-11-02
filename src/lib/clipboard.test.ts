// Copyright 2026 The Kstack Authors
//
// Licensed under the Apache License, Version 2.0 (the "License");
// you may not use this file except in compliance with the License.
// You may obtain a copy of the License at
//
//     http://www.apache.org/licenses/LICENSE-2.0
//
// Unless required by applicable law or agreed to in writing, software
// distributed under the License is distributed on an "AS IS" BASIS,
// WITHOUT WARRANTIES OR CONDITIONS OF ANY KIND, either express or implied.
// See the License for the specific language governing permissions and
// limitations under the License.

import { beforeEach, describe, expect, it, vi } from 'vitest';

const { reportError } = vi.hoisted(() => ({ reportError: vi.fn() }));
vi.mock('@/lib/error-bus', () => ({ reportError }));

const { copyToClipboard } = await import('./clipboard');

describe('copyToClipboard', () => {
  beforeEach(() => {
    vi.clearAllMocks();
  });

  it.each([
    [new Error('denied'), 'clipboard: write: denied'],
    ['denied', 'clipboard: write: denied'],
  ])('reports a rejected clipboard write', async (error, message) => {
    Object.assign(navigator, { clipboard: { writeText: vi.fn().mockRejectedValue(error) } });

    copyToClipboard('text');

    await vi.waitFor(() =>
      expect(reportError).toHaveBeenCalledWith({
        source: 'render',
        message,
        cause: error,
      }),
    );
  });
});
