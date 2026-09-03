FROM python:3.11-slim@sha256:e031123e3d85762b141ad1cbc56452ba69c6e722ebf2f042cc0dc86c47c0d8b3

ENV PYTHONDONTWRITEBYTECODE=1 \
    PYTHONUNBUFFERED=1

WORKDIR /app
COPY services/maia/requirements.txt ./
RUN pip install --no-cache-dir torch==2.8.0 --index-url https://download.pytorch.org/whl/cpu \
    && pip install --no-cache-dir --requirement requirements.txt \
    && useradd --create-home --uid 10001 maia \
    && mkdir /models /licenses \
    && cp /usr/local/lib/python3.11/LICENSE.txt /licenses/PYTHON-LICENSE.txt \
    && chown maia:maia /models

COPY --chown=maia:maia services/maia/app.py services/maia/model_artifacts.py ./
COPY runtime/THIRD_PARTY_NOTICES.md /licenses/THIRD_PARTY_NOTICES.md
COPY services/maia/licenses /licenses/maia-service
USER maia

EXPOSE 8080
HEALTHCHECK --interval=10s --timeout=5s --start-period=5m --retries=12 \
    CMD ["python", "-c", "import json, urllib.request; response = json.load(urllib.request.urlopen('http://127.0.0.1:8080/health', timeout=4)); assert response == {'ok': True, 'package': 'maia2==0.11.0', 'model': 'rapid'}"]
CMD ["python", "/app/app.py"]
