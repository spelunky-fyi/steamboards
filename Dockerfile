FROM python:3.9-slim-buster

COPY requirements.txt /steamboards/
WORKDIR /steamboards
RUN pip install -r requirements.txt
COPY . /steamboards
EXPOSE 16000

ENTRYPOINT ["python", "src/steamboards.py"]
CMD []
